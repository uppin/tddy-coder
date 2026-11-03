# 2026-11-04 — Should `choose_agent` offer the ACP agents to the PTY sandbox?

**Category:** Open decision from `acp-agents` (#800, `#agent-tools 11/12`)

**Source:** `docs/dev/1-WIP/2026-10-31-agent-tools-acp-agents.md`, § Testing Strategy
(`an_unknown_agent_id_is_refused_naming_the_registry_ids`)

## What remains

#800 adds `claude-acp` and `codex-acp` to the agent registry. The `tddy-sandbox-app` test
`an_unknown_agent_id_is_refused_naming_the_registry_ids` now lists them, because `choose_agent`
offers every registry id. Whether the PTY sandbox should offer only the PTY agents is still open.

## Current behaviour

Kept as it is. `choose_agent` offers every registry id, including the two ACP agents. The developer
decided this on 2026-11-04 instead of changing the sandbox's agent choice inside #800.

## Sweep work

Decide whether `choose_agent` offers PTY agents only. If it does, change `choose_agent` and the
registry test together. Mark the code site `TODO(acp-agents)` when that change lands.
