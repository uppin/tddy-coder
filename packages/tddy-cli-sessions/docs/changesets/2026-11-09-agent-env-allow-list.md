# 2026-11-09 — Agents start from an allow-listed environment

**Type:** Feature

`#agent-tools` 28/32 ([#836](https://github.com/uppin/tddy-coder/pull/836)).

- `spawn_rendered_main` spawns on `PtyEnvBase::Cleared`; `spawn_tool` and user terminals stay `Inherited`; `spawn_tool_on`.
- Tests: `tests/agent_env_allow_list.rs`.
