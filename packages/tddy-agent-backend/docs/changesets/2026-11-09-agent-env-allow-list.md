# 2026-11-09 — Agents start from an allow-listed environment

**Type:** Feature

`#agent-tools` 28/32 ([#836](https://github.com/uppin/tddy-coder/pull/836)).

- New `agent_env` module: `agent_child_env`, `apply_agent_child_env`, `matches_env_pattern`, `TDDY_ENV_PREFIX`; the base set is a parameter. `TDDY_CODEX_OAUTH_OUT` is the one launch-set `TDDY_*` exception.
- Headless turns, the model listing (`HeadlessModelListing.env`), the ACP worker and the conversation-id mint start from exactly the computed env. A turn's route variables replace same-named ones, as in an ACP launch; `PATH` gets the executable's directory first.
- `strip_tddy_env` (the deny-list) is deleted. Tests: `tests/agent_env_allow_list.rs`; `tddy-acp-stub` tests pass `TDDY_ACP_SCENARIO` / `TDDY_ACP_STUB_RECORD` through `AcpAgentCommand.env`.
