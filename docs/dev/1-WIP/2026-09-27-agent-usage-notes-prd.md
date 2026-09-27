# PRD — agent `usageNotes` (`agent-usage-notes`, node 3 of `subagent-control`)

**Stack:** `subagent-control` — node 3 of 5 (wave 1). Base: `feature/subagent-control/grep-context`.
**Branch:** `feature/subagent-control/agent-usage-notes`

## Problem

A specialized agent is created in the dashboard (or authored as a YAML def) with a name, label,
model, system prompt, tools — and nothing that tells an operator **how to use it**: its quirks,
its limitations, what it is good at. The system prompt is instructions to the *model*, not notes
for the human operating it. Operators keep this knowledge nowhere, so it lives in heads and is
lost between people.

## What this PR delivers

A free-text **`usageNotes`** field on specialized agents — "read this before prompting me":

> *"Best at refactoring seams, bad at API design. Refuses paths under `gen/`. Needs
> `maxTurns: 5` or it wanders. Don't ask it for summaries — it reads everything first."*

The field is stored, edited and displayed **for operators**; it is **not** injected into the
agent's system prompt and not advertised to the main agent at `subagent_new_session` (scope
decision, approved).

Threading (per discovery — web agents are model-registry **assistants**, SQLite, projected onto
`SpecializedAgentDef` at spawn):

1. `SpecializedAgentDef.usage_notes: Option<String>` (serde like `api_key`: optional,
   skip-if-none; hand-written `Debug` updated). YAML defs pick it up for free.
2. `models.proto`: `AssistantEntry`, `CreateAssistantRequest`, `UpdateAssistantRequest` gain
   `usage_notes` (Update carries it whole, per its `tools`/`replaces` precedent).
3. Registry store: SQLite `assistant.usage_notes` column + migration; `NewAssistant`,
   create/update/list read/write it.
4. Registry service: `create_assistant` / `update_assistant` / `list_assistants` handlers.
5. Projection: `assistant_to_agent_def` maps `usage_notes`.
6. Web: `CreateAssistantDialog` + `EditAssistantDialog` gain a multi-line usage-notes field
   (testids `models-create-assistant-usage-notes`, `models-edit-assistant-usage-notes`);
   `AssistantsPanel` row shows a notes affordance (truncated display, full text on demand);
   `useModelRegistryFanOut` `createAssistant`/`updateAssistant` send it.
7. Docs: `docs/ft/coder/specialized-subagents.md` YAML example gains the field.

## Acceptance criteria

1. A usage note set on create is stored, returned by `list_assistants`, and editable on update —
   through the registry service, on the wire.
2. `assistant_to_agent_def` projects `usage_notes` into the def; a YAML def with `usageNotes`
   loads it.
3. The create dialog's usage-notes field submits it; the edit dialog pre-fills and updates it;
   the panel row displays it (Cypress component tests).
4. The field is optional end to end — an assistant created without one behaves exactly as today.
5. The agent's system prompt and its conversation behaviour are bit-identical with and without
   usage notes.

## Boundaries

- Not injected into any system prompt; not on `subagent_new_session`; not on the roster's
  `SessionAgentEntry` or pickers (`SubagentInfo`) — the notes are operator documentation, not
  machine context. (Roster/picker surfacing is a future enhancement if ever wanted.)
- No changes to agent resolution, spawn, or turn behaviour.
- Ollama/YAML def authors get the field via serde only — no YAML-writing tooling.

## Dependencies

None — every surface it touches exists on `master`. Its base position in the line after
`grep-context` is order, not a behavioural dependency.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` tool-previews | `resultSummary` | nothing — order only | touch `result_summary.rs` or `MessageDescriptor` |
| `n2` grep-context | Grep context args | nothing — order only | touch `tool_grep` or `grep_limited` |

## Successor PRs

None depend on this node.
