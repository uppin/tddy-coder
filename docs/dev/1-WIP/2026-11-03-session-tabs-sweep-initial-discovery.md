# Initial discovery: `#session-tabs` sweep

**Date**: 2026-11-03
**Changeset**: [2026-11-03-session-tabs-sweep.md](./2026-11-03-session-tabs-sweep.md)

## What was read

- `packages/tddy-web/src/components/GhosttyTerminal.tsx` (lines 455-505): the setup effect (`[]`)
  calls `setReady(true)` then writes `initialContent` + `osc52Tap`; the `[initialContent, ready]`
  effect writes the same content again once `ready` flips. `ready` is set only after `termRef` is
  assigned, so the second effect always has a terminal. Removing the setup write loses nothing.
- `packages/tddy-web/src/components/sessions/useTmuxSessions.ts`: with `client === null` the listing
  effect returns early and `loading = requests !== answered` stays true for ever. `useSessionTerminals`
  answers the same situation with `setError(hostNotConnectedText(null))`.
- `NewTerminalMenu.tsx` `tmuxRows`: error wins over sessions, then loading, then empty.
- `packages/tddy-terminal-rpc/src/pty_runtime.rs`: `resolve_final_argv_env` resolves the user with
  `resolve_pty_os_user`, decides with `pty_requires_privilege_drop` (real ids) and front-loads
  `setpriv --reuid --regid --init-groups`. No refusal for a non-root daemon: the child dies inside
  the PTY.
- `packages/tddy-daemon-kernel/src/{privilege_drop,pre_exec_stages,spawn_as_user}.rs`: the listing's
  mechanism. `resolve_target_account` + `plan_privilege_change` refuse with `CannotBecomeUser`
  before any fork; `StagedCommand` installs the `pre_exec` drop over a `std::process::Command`.
- `packages/tddy-pty/src/runtime.rs` spawns with `portable_pty::SlavePty::spawn_command`.

## Findings that shape the plan

1. **A `pre_exec` kernel drop cannot be attached to the PTY child through portable-pty 0.9.**
   `CommandBuilder::as_command` is `pub(crate)`, the `SlavePty` trait exposes only `spawn_command`
   (no raw slave fd), and its internal `pre_exec` (signal reset, `setsid`, `TIOCSCTTY`,
   `close_random_fds`, umask) is not extensible. Dropping in the PTY child would mean bypassing
   portable-pty (own `openpty` + `std::process::Command` + a re-implementation of that `pre_exec`) or
   patching the crate. That is a rewrite of the spawn path of every session's main terminal, and its
   only behaviour change (root daemon, another user) runs on Linux as root, which cannot be
   exercised on this machine.
2. What *can* be unified safely is the **decision**: both paths can plan the change with the
   kernel's `plan_privilege_change_from` and refuse with the same `CannotBecomeUser` text before
   anything spawns. The `setpriv` wrapper stays as the mechanism of the drop for the root-daemon
   case, unchanged.
3. `ensure_reachable` (tmux tab pre-check in `svc_terminal_ports`) stays: the PTY refusal happens
   inside a task body after the tab was registered and surfaces as a spawn failure, while the
   pre-check refuses the RPC with `FailedPrecondition` before a handle exists.
4. `2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md` stays open: the deployment
   decision (supervisor brokers the spawn) is untouched by any of this.
