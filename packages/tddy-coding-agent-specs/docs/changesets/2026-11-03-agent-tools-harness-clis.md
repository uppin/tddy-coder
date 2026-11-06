# 2026-11-03 — OpenCode, Muse Code and Warp from their specs

**Type:** Feature

`#agent-tools` 8/12 ([#797](https://github.com/uppin/tddy-coder/pull/797)).

- `specs/{opencode,muse,warp}.yaml`.
- `CodingAgentSection` gains `route` (`RouteSlot { required_in_jail, config_env, key_env, files_dir }`), `models.{list_command, adapter}`, `resume` (`ResumeSlot { adapter }`) and `placements` (`CodingAgentSpec::require_placement`).
- `McpSlot::Unsupported`, the `unsupported` hooks adapter (`SlotError::HooksUnsupported`; a stand-in until `hooks: unsupported`), and `spec_for_session_type`.

See [OpenCode](../../../../docs/ft/daemon/opencode-cli-session.md), [Muse Code](../../../../docs/ft/daemon/muse-cli-session.md) and [Warp](../../../../docs/ft/daemon/warp-cli-session.md) CLI sessions.
