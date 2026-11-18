# 2026-11-18 — `launch_on_target` is cut into its steps

**Type:** Refactor · `#coder-start` 4/4, PR [#944](https://github.com/uppin/tddy-coder/pull/944)

`launch_on_target` in `src/remote_agent_launch.rs` went from 184 to 59 lines through eight private
helpers, applied through `tddy-tools restructure`: `plan_the_launch`, `write_slot_files`,
`write_rendered_session_files`, `remote_command`, `client_command`, `client_argv_and_env`,
`judge_the_start` and `recorded_owner`, each at most 48 lines. No behaviour, signature or public-surface
change.

Left open: the `launch_on_target` code issue is narrowed to its six parameters (budget 5); bundling them
changes the public signature, so it is a feature change, not a restructure. The pid-directory `exec`
stays inline: rust-analyzer's extraction of it rewrote `ExecRequest::new(format!(..))` into a form that
does not compile.

Full record: [`docs/dev/changesets/2026-11-18-coder-start-sweep.md`](../../../../docs/dev/changesets/2026-11-18-coder-start-sweep.md).
