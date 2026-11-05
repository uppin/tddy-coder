# 2026-11-05 — Reserved assistant names come from the coding-agent registry

**Type:** Refactor

`#agent-tools` 16/19 ([#822](https://github.com/uppin/tddy-coder/pull/822)).

- `ModelRegistryStore::reserving_coding_agent_ids(ids)`: an assistant named like any of them is refused as `'<name>' is a coding backend`. Replaces the read of `tddy_coder::run::BUILTIN_BACKEND_AGENT_IDS` (seven ids, no `oz`, no dropped-in agent); the crate's `tddy-coder` dependency is gone. `tddy-daemon`'s `runtime/mount_host.rs` feeds it `tddy_coder::agent_registry_cli::reserved_agent_ids` of the host's coding agents.
- Tests: `tests/model_registry_reserved_names_unit.rs` (a dropped-in agent and `oz` refused, an unknown name accepted).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-spec-backend.md).
