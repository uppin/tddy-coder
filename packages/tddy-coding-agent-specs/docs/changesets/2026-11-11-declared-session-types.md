# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- `extensions.session { session_type, launch, label }`, owned by `SessionExtension` (`session.rs`): `SessionSection`, `LaunchKind::{AgentCli, TddyCoder}`, `DeclaredSessionType`; `NO_AGENT_SESSION_TYPE` / `is_no_agent_session_type` re-exported from `tddy-workflow`.
- Every PTY built-in spec declares its session type (`claude-cli`, `codex-cli`, `cursor-cli`, `muse-cli`, `opencode-cli`, `oz-cli`, `warp-cli`); `session-tools/tddy-coder.yaml` declares `tool` (`builtin_session_tools()`, `TDDY_CODER_SPEC_YAML`).
- `CodingAgentRegistry`: `session_type`, `session_types`, `session_type_of`, `sole_session_type_launching`, `offered_session_types`, `undeclared_session_type_message`; `for_session_type` reads the declaration. `CLI_SESSION_TYPE_SUFFIX` removed.
- Load refusals: `RegistryError::SessionType` (invalid section, `agent_cli` on a non-PTY tool, declared `workspace`), `RegistryError::DuplicateSessionType` (naming both files), and `load_agent_registry` refusing an override that drops its built-in's session section.
- Docs: [coding-agent-specs § Session types](../coding-agent-specs.md#session-types).

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
