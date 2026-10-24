# 2026-10-24 — A stalled or impossible spawn says why
**Type:** Fix

`#start-progress` 1/8 ([#734](https://github.com/uppin/tddy-coder/pull/734)).

A clone child forked by an unprivileged worker hung forever before `exec`: its `pre_exec` called
`std::env::set_var` (std holds the env lock across `fork`) and `initgroups(3)` (NSS after `fork`), and
nothing logged where it was. Across:

- `tddy-daemon-kernel` — pre-fork account and group resolution, the effective-id privilege plan with an
  up-front refusal (`CannotBecomeUser`), and `pre_exec_stages::StagedCommand` (async-signal-safe drop,
  stage pipe, per-step and heartbeat logs). `as_user_command` moved onto it.
- `tddy-spawn` — `clone_as_user_with_env` and `spawn_as_user` moved onto it; three hand-written closures
  deleted.
- `tddy-agent-launch` — `ensure_project_available_for_start` logs the clone's failure and deadline arms.

Every spawn now forks, same-user ones included. Tests: `tddy-daemon-kernel/tests/privilege_change_acceptance.rs`,
`tests/pre_exec_stages_acceptance.rs`; `tddy-spawn/tests/clone_privilege_acceptance.rs`, `tests/pre_exec_safety.rs`.

Code issues: `tddy-agent-launch` `complexity-…-ensure-project-available-for-start` regressed 99 → 108
lines, deferred with the developer's consent. Daemon-kernel's two `privilege_drop` records touched,
unchanged.
