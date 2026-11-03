# 2026-11-03 — The PTY spawn refuses like the tmux listing

**Type:** Bug Fix

`pty_runtime::resolve_final_argv_env` plans the change with `tddy_daemon_kernel::privilege_drop::plan_privilege_change_from`: a non-root daemon asked for another user is refused with the kernel's `CannotBecomeUser` before a PTY opens; a root daemon still drops behind `setpriv`; the same user is untouched. Three new `pty_runtime` units. Code issue `oversized-file-service` re-measured unchanged (546), deferred. Part of [#813](https://github.com/uppin/tddy-coder/pull/813) — see [Terminal sessions](../../../../docs/ft/daemon/terminal-sessions.md).
