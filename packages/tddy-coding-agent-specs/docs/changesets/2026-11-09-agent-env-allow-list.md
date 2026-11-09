# 2026-11-09 — Agents start from an allow-listed environment

**Type:** Feature

`#agent-tools` 28/32 ([#836](https://github.com/uppin/tddy-coder/pull/836)).

- `coding_agent.pass_env` (names and `PREFIX_*`); an entry admitting `TDDY_*` is a load error naming the spec.
- Built-in specs declare provider keys: claude/claude-acp `ANTHROPIC_API_KEY`, codex/codex-acp `OPENAI_API_KEY`, cursor `CURSOR_API_KEY`, opencode both; muse/warp/oz none.
- `AgentEnvBase::{builtin, load, parse, agent_env}`: the base set is YAML (`specs/agent-env.yaml`, replaced whole by `<tddyhome>/tools/agent-env/base.yaml`); `TDDY_*` and a `schema_version` other than 1 are load errors naming the file. `CodingAgentRegistry::agent_env_base`.
- Tests: `tests/pass_env_schema.rs`, `tests/agent_env_base_load.rs`.
