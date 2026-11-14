# 2026-11-13 — Orchestrators drive a session's turns

**Type:** Feature

`#orchestrators 4/11`, PR [#881](https://github.com/uppin/tddy-coder/pull/881), stack
[#889](https://github.com/uppin/tddy-coder/stack/889). Feature:
[project-concept.md § Sending a session a turn](../../ft/daemon/project-concept.md#sending-a-session-a-turn).

## What changed

- **`tddy-coding-agent-specs`** — `coding_agent.turn_input` (`TurnInputSlot::{TerminalPaste { submit_key }, AcpPrompt}`), validated at load against the terminal key-name table, which moved here (`terminal_keys::{named_key_bytes, is_key_name}`); `claude.yaml` declares `terminal_paste` + `Enter`.
- **`tddy-tool-engine`** — `project_home_turn_tools.rs`: `SendSessionMessage`, `ResumeSession`, `ProjectHomeTurnPort`, `TurnDelivery`, `ResumeOutcome`; registered as project-home and target tools. The inbox tools take `started_by_me`. `terminal_keys::encode_keys` reads the key table from the specs crate.
- **`tddy-daemon-rpc`** — `exec_tool/project_home/turn.rs`, the port's daemon impl: terminal paste plus submit key without a lease change; resume through `resume_session_at_session_coordinate`, then the first message once the main terminal draws. `scope_of(home, started_by_me)`.
- **`tddy-session-activity`** — `ProjectNotificationRecord.orchestrator_session_id` (from the session's changeset at append, omitted from the JSON when `None`); `ProjectNotificationScope.started_by`.
- **`tddy-tools`** — the project-home admission test lists the two tools.

## Deferred

- `acp_prompt` delivery: the daemon refuses it (`TODO(acp-turn-input)`); developer decision 2026-11-12, recorded in `docs/dev/todo/2026-11-13-acp-turn-input-is-not-delivered.md`.

## Code issues

- `tddy-tool-engine/src/lib.rs` `oversized-file`: 889 → 890 production lines (one `pub mod`). Not split: stack-shared with #882 and #883.
