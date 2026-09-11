# The live agent roster and the subagent conversation runtime

## Purpose

Two public modules serve everything a session does with its specialized agents. **`roster`** is the
in-jail registry that follows the session's live agent roster over `StreamSessionAgents`: which
agent a call may address, which exec tools an attached agent has taken over, and the conversation
RPCs that reach an agent this process holds no def for. All of those are addressed at
`session_agents.SessionAgentService` — the coordinate `#unbundle` node 7 moved family B to, served
by `tddy-session-agents` in front of the runtime here — read from
`tddy_service::session_agents::SESSION_AGENT_SERVICE` so this client and that server cannot disagree
about the name. **`subagent_runtime`** is the table of open conversations, the turns running on
them, and what each has spent.

This crate owns them because it already owns every type they manipulate — `subagent`'s session
traits, `agent_def::SpecializedAgentDef`, `openai::TokenUsage` — plus the two dependencies neither
`tddy-core` nor `tddy-service` can carry: `tddy-service`'s `session_agents.proto` types (which
`tddy-service` is *below*, so it cannot hold `registry`) and `tddy-session-tool-client` (how a jail
reaches its facilitating daemon).

## What is here and what is not

| Here | In `tddy-tools` |
|---|---|
| `LiveAgentRoster` and its four-state currency, the seed, the subscription and its backoff | the `subagent_*` MCP tool bodies, their input schemas, and the `ToolRouter` they are advertised through |
| the conversation table, `PendingTurns`, token accounting, the status report to the facilitating daemon | `subagent_new_session_schema`'s roster-derived `enum` and the JSON shapes the tools answer in |

The line is the one every seam in `#unbundle` node 5 was drawn on: **the shape of an MCP tool
belongs to the crate that speaks MCP**, and the logic behind it belongs to the crate that owns the
types. `tddy-tools` reaches this module as `tddy_tools::session_agents`, the path it was reached by
before it moved.

## Where the design is written down

The module docs carry it, and they are the authority rather than a summary of one:

| Module | What it is answerable for |
|---|---|
| `roster` (`src/roster.rs`) | the two properties the registry exists to hold — a frame *replaces* rather than merges, and a registry that cannot be kept current *refuses* — and the bound on the second: a withdrawal never outlives the reachability of its replacement |
| `roster::registry` | what the roster currently says and what it permits |
| `roster::seed` | what the spawn environment claimed, deliberately the weakest input |
| `roster::stream` | whether to subscribe at all, and one subscription reconnected forever |
| `roster::conversation` | running an agent this process holds no def for, by asking the facilitating daemon to run it |
| `roster::link` | which daemon those RPCs reach, and the identity they carry |
| `subagent_runtime` | a conversation outlives the `tools/call` that opened it; a turn outlives the call that started it |

**`RosterCurrency` is private, and that is a decision.** It has four states — `Seeded`,
`Current`, `Stale`, `Unreachable` — and only some of them may still enforce a tool withdrawal.
Callers read the *answers* instead: `RosterStatusReport::{applied_rev, refusal}` and the refusals
from `resolve` and `check_tool_available`. Publishing an internal state machine would make those
four states contract.

## Features

`livekit` is **off by default** and forwards to `tddy-session-tool-client`'s. A jail that reaches its
daemon over the in-jail socket links no LiveKit SDK to follow a roster; `tddy-tools`' own default-on
feature turns it on.

## Testing

The heaviest coverage is not in this crate and deliberately stays where it is: `tddy-tools`'
`session_agent_roster_client_acceptance` (48), `session_agent_conversation_client_acceptance` (13),
`subagent_status_wait_acceptance` (14), `subagent_tool_advertisement_acceptance` (8) and
`subagent_async_response_acceptance` (13) drive the real `--mcp` stdio wire with `assert_cmd`, so
they prove the runtime works *wherever* it is implemented. Rebuilding them around a library call
would have thrown that property away. The unit tests that exercise the conversation table moved with
it.

## Related

- [Session agent roster](../../../docs/ft/daemon/session-agent-roster.md) — the product contract, § Invoking an agent and § The roster stream
- [Sandboxed codebase mode](../../../docs/ft/coder/sandboxed-codebase-mode.md) — the jail this runs inside
- [`tddy-session-tool-client`](../../tddy-session-tool-client/README.md) — the transport underneath
- [`tddy-tools`](../../tddy-tools/README.md) — the MCP surface that drives both modules
