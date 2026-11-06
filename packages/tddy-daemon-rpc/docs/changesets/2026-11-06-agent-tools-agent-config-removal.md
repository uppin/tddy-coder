# 2026-11-06 — The model probe resolves through the host's coding agents

**Type:** Fix

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)).

- `CatalogRpcHandler` holds `CatalogAgentSources { tddy_data_dir, model_registry, coding_agents }`; `listing_binary` resolves the probed agent's binary through the host's own handle instead of reloading the tool files per probe (which ignored installed agents). Field budget (5) kept.
- Fixtures: launchers given `builtin_coding_agents()`; `claude_cli:` stubs moved to `claude.yaml`'s `binary.env`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-agent-config-removal.md).
