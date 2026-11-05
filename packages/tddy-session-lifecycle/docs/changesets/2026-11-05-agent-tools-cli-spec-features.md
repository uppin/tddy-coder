# 2026-11-05 — Acceptance for the generic CLI launch's claude and cursor features

**Type:** Test

`#agent-tools` 14/19 ([#820](https://github.com/uppin/tddy-coder/pull/820)).

- `tests/cli_spec_features_acceptance.rs` (new) and `tests/yaml_only_agent_acceptance.rs`: copies of `claude.yaml` / `cursor.yaml` and dropped-in agents through the generic launch — tmux, project-home main, ssh target, managed workflow with hooks, both conversation kinds, the mint and ssh deadlines, the jailed claude copy, and no `TDDY_*` variable from the daemon on the host, in tmux, over ssh or in a jail.
- `tests/agent_registry_support`: `builtin_yaml_as`, `cursor_copy_minting_within`, `StubCreateChat`, `ATddyVariableInTheDaemon`, the tmux / managed / ssh start builders, and the stub's `TDDY_*` report.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-cli-spec-features.md).
