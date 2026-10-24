# 2026-10-24 — A stalled or impossible spawn says why
**Type:** Fix

`#start-progress` 1/8 ([#734](https://github.com/uppin/tddy-coder/pull/734)).

`clone_as_user_with_env` and `spawn_as_user` start their child through the kernel's `StagedCommand`;
their passwd lookups, local `TargetAccount` and `set_var` / `initgroups` `pre_exec` closures are
deleted. See [spawning-as-another-user.md](../spawning-as-another-user.md).

Code issues: this crate has no `docs/code-issues/`; `spawner.rs` only lost code (see
`docs/dev/todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md`).
