# Spawning as another OS user

`clone_run::clone_with_progress` (behind `spawner::clone_as_user_with_env`) and `spawn_as_user` (`src/spawner.rs`) start their child through
`tddy_daemon_kernel::pre_exec_stages::StagedCommand`; neither writes its own `pre_exec`. The account is
resolved with `tddy_daemon_kernel::privilege_drop::resolve_target_account` (wrapped here as an `anyhow`
error) and the privilege change planned with `plan_privilege_change` before any fork, so a non-root
process asked for another user fails with `CannotBecomeUser` — for a clone, before the destination is
created. The mechanism, the log lines and its edge cases are documented in
[daemon-kernel.md § `privilege_drop` and `pre_exec_stages`](../../tddy-daemon-kernel/docs/daemon-kernel.md#privilege_drop-and-pre_exec_stages--starting-a-child-as-another-os-user).

- The watcher labels are `clone <destination>` and `session <session_id>`.
- A clone returns early, without a child, when the destination exists. How it runs, reports progress and
  stops: [clone-run.md](clone-run.md).
- Extra clone environment (`GIT_SSH_COMMAND`, `TDDY_*`) travels in the child's own environment; the
  privilege drop does not touch it.
- `SessionChildPlan` keeps the kernel `TargetAccount`: the forking backend plans its privilege change
  from it, and the session's files are chowned to it.

`tests/pre_exec_safety.rs` fails if `set_var(` or `initgroups(` appears in `spawner.rs` or the kernel's
`spawn_as_user.rs`, or if either stops naming `StagedCommand`. `tests/clone_privilege_acceptance.rs`
asks for a clone as `root` from the unprivileged test process.
