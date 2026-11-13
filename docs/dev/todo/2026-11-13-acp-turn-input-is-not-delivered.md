# An agent declaring `turn_input: acp_prompt` is refused every message

**Recorded:** 2026-11-13, by `#orchestrators 4/11` (PR #881).

## What is wrong

`coding_agent.turn_input` has two kinds. `terminal_paste` is delivered by the daemon
(`SendSessionMessage`, and `ResumeSession`'s first message). `acp_prompt` parses and validates at
load, and the tool-engine layer answers `delivered_via: acp_prompt` against a fake port, but the
daemon's port refuses it: it holds no route from a project-home tool call to a running session's ACP
connection. A `ResumeSession` with a message to such an agent is refused **before** the session is
resumed, so nothing is left half-done.

No shipped agent YAML declares `acp_prompt` today, so nothing reachable is broken.

## Developer decision

2026-11-12 (6e review of #881): ACP turn input is covered by the tool-engine unit test only, until an
ACP agent's YAML declares it. Do not implement it in `#orchestrators`.

## What would close it

The first ACP agent YAML to declare `turn_input: { kind: acp_prompt }`, together with a daemon route
from `DaemonProjectHome` to that session's ACP connection (send a `session/prompt`), and a daemon-rpc
acceptance test sending such a session a turn.

## Where

- `packages/tddy-daemon-rpc/src/exec_tool/project_home/turn.rs` — `terminal_submit_key`,
  `TODO(acp-turn-input)`
- `packages/tddy-coding-agent-specs/src/spec/turn_input_slot.rs` — `TurnInputSlot::AcpPrompt`
