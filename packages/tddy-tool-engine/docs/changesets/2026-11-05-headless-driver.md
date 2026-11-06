# 2026-11-05 — An exec-tool command starts with no TDDY_* variable

**Type:** Fix

`#agent-tools` 15/19 ([#821](https://github.com/uppin/tddy-coder/pull/821)).

- `contained_shell::run_contained_argv` removes every `TDDY_*` variable from a spawned command's environment, inherited from the host process or passed in the engine's map, as it removes the GitHub token variables. A `tddy-tools` the command runs finds its session through the `.tddy/session.json` pointer.
- Tests: `tests/exec_shell_no_tddy_env.rs`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md).
