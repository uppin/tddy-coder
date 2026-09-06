# 2026-08-13 — tddy-daemon — generalize `pr_stack_spawn_args` to all optional spawn flags

**Category:** Future enhancement
**Source:** pr-stack-base-session changeset, 2026-08-13

`spawner::pr_stack_spawn_args` exists because an argument vector can be asserted on where a `Command`
cannot. That instinct applies to the four hand-rolled trim/skip-if-empty/`cmd.arg` blocks immediately
above it (agent, recipe, model, project id): `spawn_as_user` now has two mechanisms for one job.
Renaming it to an `optional_flag_args(&[(&str, Option<&str>)])` and routing those four through it
collapses their per-flag `log::debug!` lines into one and makes them testable too.
