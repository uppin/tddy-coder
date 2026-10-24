# 2026-10-24 — A stalled or impossible spawn says why
**Type:** Fix

`#start-progress` 1/8 ([#734](https://github.com/uppin/tddy-coder/pull/734)).

- `privilege_drop`: `TargetAccount`, `resolve_target_account` (passwd + `getgrouplist`, before fork),
  `EffectiveIdentity`, `PrivilegeChange`, `plan_privilege_change` / `plan_privilege_change_from`,
  `CannotBecomeUser`.
- New `pre_exec_stages`: `PreExecStage`, `stage_record`, `STAGE_HEARTBEAT`, `watch_stages`,
  `StageOutcome`, `StagedCommand`, `StagedChild`.
- `spawn_as_user::as_user_command` uses them; its `set_var` / `initgroups` closure is gone, and identity
  is compared by effective ids. See [daemon-kernel.md](../daemon-kernel.md#privilege_drop-and-pre_exec_stages--starting-a-child-as-another-os-user).

Deviations from the planned contract: `TargetAccount.name` is a `String` (the child never needs it);
`watch_stages` takes no pid (each record carries it); a non-root process with a matching uid but another
gid is refused too.

Code issues: `misplaced-tests-privilege-drop` and `missing-tests-privilege-drop-resolve-pty-os-user`
touched, unchanged (rows added). None deleted.
