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
| `subagent::turn_request` | what a caller may ask of a turn — a new question, a continuation, a rewind, a correction — and the budget it runs under, including why the **floor** is as load-bearing as the ceiling |
| `subagent::transcript` | one conversation's addressable history: id minting that never reuses a discarded id, preview truncation, and rewind boundary-snapping that never separates a tool call from its results |
| `subagent_runtime` | a conversation outlives the `tools/call` that opened it; a turn outlives the call that started it |

## One entry point per turn, and one rule the loop will not break

`SubagentSession` has a single turn method, `take_turn(TurnRequest)`. The request is built from one
of two starting points so the intents cannot be confused — `TurnRequest::prompting` asks something
new, `TurnRequest::resuming` carries on what is already there — and the builders narrow it from
there (`from_message`, `with_correction`, `within_turns`). Both implementors take it: the in-process
`SpecializedSubagentSession` and the `RemoteAgentSession` that asks the facilitating daemon to run
the loop.

Two rules the runtime holds regardless of implementor:

- **A budget spent entirely on tool calls of which none ran is an error, returned before any model
  call.** The synthesis turn asks a model to summarise "citing the specific file:line locations you
  found" and has nothing that checks anything was found; at `temperature: 0.0` a model obliges
  deterministically. The guarantee is that nothing was asked to summarise, not that its answer was
  discarded. The conversation is left exactly as the failed call built it and stays promptable and
  rewindable. **The guard sits on the budget-exhaustion path alone** — a model that ends the turn
  early after every tool call failed still returns its answer
  ([`docs/dev/todo/2026-09-26-the-honest-failure-guard-does-not-cover-a-model-ended-turn.md`](../../../docs/dev/todo/2026-09-26-the-honest-failure-guard-does-not-cover-a-model-ended-turn.md)).
- **A caller's budget is bounded; a definition's is not.** `1..=50`, clamped to the nearer bound and
  the clamp reported in `PromptOutcome::clamped_max_turns`. A definition's own figure is the
  operator's configuration and stands as written. The floor exists because the loop runs
  `0..turns`: a zero budget runs no turn, so no tool call can fail, so the outage guard has nothing
  to fire on and control reaches the synthesis turn with a history holding only the prompt.

`RemoteAgentSession` refuses one shape the local session accepts: a turn carrying **both** a new
prompt and a rewind point or correction. No MCP caller can construct it, so no wire field was
invented for it.

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

The turn-control behaviour is covered **here**, against a `wiremock` provider, because the strongest
assertion available is *the request body the model receives*: that no synthesis request was sent (an
exact request count, never a content check — a presence check would pass if synthesis ran and
happened to fail), that a rewind truncated the history to the right prefix (compared element-wise,
never by length), that a correction is one message in final position, and that a clamped budget
produced exactly *N* requests. `subagent_tool_outage_red.rs`, `subagent_message_ids_red.rs`,
`subagent_resume_red.rs` and `subagent_turn_budget_red.rs` are those suites;
`subagent_context_exhaustion_red.rs` established the pattern.

Several of them assert an **absence** — no synthesis request, no extra message. Those assertions
have no positive control that would fail if the harness stopped driving the code at all; recorded
in
[`docs/dev/todo/2026-09-26-five-absence-assertions-in-the-new-subagent-suites-have-no-positive-control.md`](../../../docs/dev/todo/2026-09-26-five-absence-assertions-in-the-new-subagent-suites-have-no-positive-control.md).

## Related

- [Session agent roster](../../../docs/ft/daemon/session-agent-roster.md) — the product contract, § Invoking an agent and § The roster stream
- [Sandboxed codebase mode](../../../docs/ft/coder/sandboxed-codebase-mode.md) — the jail this runs inside
- [`tddy-session-tool-client`](../../tddy-session-tool-client/README.md) — the transport underneath
- [`tddy-tools`](../../tddy-tools/README.md) — the MCP surface that drives both modules
