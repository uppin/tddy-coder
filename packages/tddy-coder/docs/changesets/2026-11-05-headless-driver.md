# 2026-11-05 — The claude, cursor and codex workflow backends from their specs

**Type:** Refactor

`#agent-tools` 15/19 ([#821](https://github.com/uppin/tddy-coder/pull/821)).

- `headless_from_spec::headless_backend_from_spec(agent, tools_dir, host)` builds a `SpecHeadlessBackend` from the registry's spec (renderer, protocol, `routed_agent`, model listing); `create_backend`'s `claude`, `cursor` and `codex` arms use it. `claude_from_spec.rs` is folded into `headless_from_spec.rs`.
- Tests: `tests/spec_headless_backend.rs` — the four built-ins and a YAML-only agent through their real specs against stub CLIs (argv, MCP files and their removal, session ids, failure texts, no `TDDY_*` in the agent's environment, the session pointer).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md).
