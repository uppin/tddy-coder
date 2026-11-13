# 2026-11-13 — Orchestrators drive a session's turns

**Type:** Feature

`#orchestrators 4/11` (PR [#881](https://github.com/uppin/tddy-coder/pull/881)). Cross-package: [session turn control](../../../../docs/dev/changesets/2026-11-13-session-turn-control.md). Product: [Sending a session a turn](../../../../docs/ft/daemon/project-concept.md#sending-a-session-a-turn).

- `coding_agent.turn_input` (`spec/turn_input_slot.rs`, `TurnInputSlot`, `CodingAgentSpec::turn_input`), checked at load by `validate_turn_input_slot`; `claude.yaml` declares `{ kind: terminal_paste, submit_key: Enter }`.
- `terminal_keys` — the key-name table (`named_key_bytes`, `is_key_name`), moved from `tddy-tool-engine`.
- Docs: [coding-agent-specs § Turn input](../coding-agent-specs.md#turn-input-coding_agentturn_input).
