# 2026-11-07 — The model probe lists from the host's registry

**Type:** Fix

`#agent-tools` 22/28 ([#830](https://github.com/uppin/tddy-coder/pull/830)).

- `list_models_probe_args(agent, binary_path, tools_dir)` always appends `--tools-dir <tddyhome>/tools`, so `ListAgentModels` returns an operator's `tools/<agent>.yaml` model override rather than the built-in list.
- Fixtures construct `DaemonSessionHost` with their registry.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-one-host-coding-agent-registry.md).
