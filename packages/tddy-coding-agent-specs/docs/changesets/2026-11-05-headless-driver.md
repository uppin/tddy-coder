# 2026-11-05 — The headless protocol fields

**Type:** Feature

`#agent-tools` 15/19 ([#821](https://github.com/uppin/tddy-coder/pull/821)).

- `HeadlessSection` gains `system_prompt` (`flag` | `prepend`), `goal_params` (`always`, `plan_mode`, `read_only_plan`, `accept_edits`), `approval_tool`, `workflow_mcp`, `route_model` (`{ kind: kept | replaced | replaced_if_route_has, flag }`), `fresh_session` (`caller` | `agent`) and `scratch` (`session_dir` | `workdir`); an unknown word is refused at load naming the file and the word. `HeadlessSection::protocol()`; the types are re-exported from `tddy_agent_backend::adapters::headless`.
- `SpecHeadlessRenderer::{protocol, model_listing}`; a goal's allowed tools are not passed to an agent whose `tool_allow` is `unsupported`.
- `claude.yaml`, `cursor.yaml`, `codex.yaml` (with `headless.env` `BROWSER: "{self_exe}"`, `TDDY_CODEX_OAUTH_OUT`) and `oz.yaml` declare the protocol their workflow turns run with. The render goldens are unchanged.
- Tests: `tests/headless_protocol_spec_unit.rs`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md).
