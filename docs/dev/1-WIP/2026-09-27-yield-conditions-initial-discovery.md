# Whole-work initial discovery — `subagent-control` stack

**Stack:** `subagent-control` — 5 nodes: `tool-previews` (n1), `grep-context` (n2),
`agent-usage-notes` (n3), `yield-conditions` (n4), `resume-replacement` (n5).

## Combined conclusions

- **The turn loop's single hook point is `run_one_turn`'s tool-call inner loop** (`subagent.rs:1405-1416`):
  after `dispatch_bounded` → `transcript.push_marked(...)` — where result summaries are computed,
  where yield conditions are checked, and where a yielded turn stops. Everything else is plumbing
  around that one site.
- **`MessageDescriptor` is the serialized surface for summaries** (`transcript.rs:107-123`), widened
  both on the MCP JSON (`prompt_outcome_json`) and on the proto final frame
  (`AgentMessageDescriptor`, `session_agents.proto:120-128`) via `message_descriptor`
  (session-agents service.rs:439) and `parse_message_descriptor` (roster/conversation.rs:407) — the
  latter already carries a TODO to widen names-only `tool_calls`.
- **STR_REPLACE has no matched-lines count today** — the engine counts occurrences internally only to
  enforce uniqueness (`tool_str_replace`, `tddy-tool-engine/src/lib.rs:384-390`) and does not return
  the count. `resultSummary`'s `{matchedLines}` and yield predicate `{outcome:{matchedLines:0}}`
  both need the engine to expose it. STR_REPLACE is Managed-only (Local path rejects writes), so
  there is one place to add it.
- **A yield condition needs a new `StopReason` variant** with **5 exhaustive sites**
  (`turn_stop_reason`, `agent_stop_reason` service.rs:365, `parse_stop_reason`
  conversation.rs:460, `prompt_outcome_json`, plus the proto string spelling) — proto `stop_reason`
  is string-typed, so no proto enum to extend, but both spellers must agree. Do **not** touch the
  unrelated ACP `StopReason` enums (tddy-acp / model-registry acp_service).
- **Replacement resume is a transcript-append of synthetic messages**: `Transcript` has only
  `push`/`push_marked`; there is no append-a-tool-call-and-result path. `ChatMessage::assistant`
  (tool_calls) and `ChatMessage::tool_result(content, tool_call_id, tool_name)` constructors exist
  (used at subagent.rs:1401/1409). The resume "correction" path appends only a `user`-role text
  message (subagent.rs:1745-1748) — the chosen keep-original + append design extends exactly this
  site.
- **`usageNotes` threads through the model registry, not YAML**: web-created agents are SQLite
  "assistants" projected onto `SpecializedAgentDef` (`assistant_to_agent_def`). Threading:
  agent_def.rs struct + hand-written Debug → models.proto (AssistantEntry/Create/Update) →
  registry store (SQLite column + migration, NewAssistant, create/update/list) → service handlers →
  projection → web dialogs + useModelRegistryFanOut → Cypress → docs. YAML defs pick it up via
  serde (`api_key` precedent: optional, skip-if-none).
- **Grep context lines**: `tool_grep` (`tddy-tool-engine/src/lib.rs:436`) shells `rg --json -e
  <pattern> .` keeping only `type:"match"` events; adding `before`/`after` args means passing
  `-B/-A` to rg and folding `type:"context"` events into the result. Discovery-side
  `grep_limited` (subagent.rs:339) forwards `Grep` args to the engine (Managed) and also implements
  a Local path (`grep_file`/`grep_dir`) — **both paths need the context args**. Catalog schema
  (catalog.rs:44) + subagent tool schema + `validate_tool_arguments` all gain the fields.
- **Step 2b verdicts**: oversized-file code issues on subagent.rs / subagent_runtime.rs / server.rs
  / service.rs / tool-engine lib.rs (all open, unclaimed, restructure-required) → new code goes in
  **new modules**, not as growth of those files. The turn-outcome frame-size TODO
  (`a-turns-message-list-can-overflow…`) constrains n1: `resultSummary` adds bytes to every
  descriptor on the final LiveKit frame — keep summaries small and count them against the 48 KiB
  frame budget. The resume-RPC wire-test TODO is n5's to close with wire-level tests.
- **Stale 1-WIP leftovers**: `2026-09-20-specialized-agent-context-handoff.md` (PR #521, merged —
  `ContextExhausted` is live) and `2026-09-20-subagent-turn-queue-visibility.md` are not active
  conflicts. Open PR #548 (`sandbox-tool-specs`, docs-only requirements) precedes a jail tool-set
  change that will touch the same catalog — n2 notes it as a watch item.

## Exploration N

## Exploration 1 — Agent defs, roster, and the web agent-creation flow

**Key architectural finding: the web does not create/edit `SpecializedAgentDef` YAML files.**
Web-created agents are "assistants" — rows in the model registry's SQLite store — projected onto
`SpecializedAgentDef` at resolution time (`packages/tddy-model-registry/src/assistant_def.rs`,
`assistant_to_agent_def`). A `usageNotes` field must therefore be threaded through the def struct,
the model-registry assistant model (proto, store, service, projection), and the web assistant
dialogs — while YAML defs pick it up via serde.

### `SpecializedAgentDef` (`packages/tddy-discovery/src/agent_def.rs:105-141`)

```rust
pub struct SpecializedAgentDef {   // #[serde(deny_unknown_fields)]
    pub name: String,              // registry key
    pub label: Option<String>,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub system_prompt: Option<String>,
    pub system_prompt_path: Option<PathBuf>,
    pub tools: Vec<SubagentTool>,   // default Read/Glob/Grep
    pub max_turns: u32,            // default 10
    pub replaces: Vec<String>,
}
```

- YAML files under `<tddyhome>/agents/*.yaml` (`load_agent_defs`, agent_def.rs:168-195). Nothing in
  the repo writes them — operator-authored. A new optional field follows the `api_key` serde
  precedent (`#[serde(default, skip_serializing_if = "Option::is_none")]`).
- Hand-written `Debug` (agent_def.rs:147-162) redacts `api_key` — a new field must be added there too.

### Roster and wire types

- `SessionAgentRecord` (`packages/tddy-changeset/src/session_agent.rs:113-135`): agent_id, name,
  daemon_instance_id, label, model, replaces, tools, codebase_session_id — snapshotted roster entry
  persisted in `.session.yaml`.
- Roster wire `SessionAgentEntry` (`packages/tddy-service/proto/session_agents.proto:288-318`) plus
  `clone_state`/`clone_error`/`status`; generated TS `packages/tddy-web/src/gen/session_agents_pb.ts`.
- Def → roster record: `packages/tddy-session-agents/src/agent_records.rs:21-36` (`roster_record`);
  def → picker row: `packages/tddy-daemon-rpc/src/catalog/subagent_row.rs:10-27` →
  `catalog.SubagentInfo` (`catalog.proto:73-90`). If `usageNotes` should surface in pickers, both
  need it.
- Spawn resolution: `packages/tddy-session-agents/src/spawn_agent_def.rs:3-24` — registry assistants
  win over YAML defs of the same name.

### Web "Models & Agents" screen (all in `packages/tddy-web/src/components/models/`)

- `CreateAssistantDialog.tsx` (:34-76; fields name, label, systemPrompt, tools, replaces;
  testids `models-create-assistant-*`), `EditAssistantDialog.tsx` (:28-33; label, systemPrompt,
  tools, replaces), `AssistantsPanel.tsx` (row display), `ModelsScreen.tsx` (:56-64),
  `ModelsAppPage.tsx` (:95/:106 wire to registry client).
- TS RPC client `useModelRegistryFanOut.ts` — `createAssistant` :509-531, `updateAssistant`
  :533-553.
- Cypress: `cypress/component/models/{AssistantsPanel,AssistantEditing,AssistantTakeover}Acceptance.cy.tsx`,
  support `rpc/modelRegistryBackend.ts`, `pages/modelsScreenPage.ts`, `testIds.ts`.

### Daemon-side create/update path (where a new field threads)

- `packages/tddy-service/proto/models.proto` — `AssistantEntry` :115-132, `CreateAssistantRequest`
  :213-223, `UpdateAssistantRequest` :228-237 (Update carries tools/replaces whole — a new field
  follows the same rule).
- `packages/tddy-model-registry/src/service.rs` — `create_assistant` :258-282,
  `update_assistant` :284-306, `list_assistants` :249-256.
- `packages/tddy-model-registry/src/store.rs` — `NewAssistant` :62-77, `create_assistant`
  :465-537 (SQLite `INSERT` :501-523 — **needs a schema migration/column**), `update_assistant`
  :582-615, `list_assistants` :540-564.
- Projection: `assistant_def.rs` — `assistant_to_agent_def` :24-60 (where
  `usage_notes: assistant.usage_notes` lands), `registry_agent_defs` :68-88.
- Wiring: `packages/tddy-daemon/src/runtime.rs:974-978`, :1285-1299.

### Docs

- `docs/ft/daemon/session-agent-roster.md` (roster PRD), `docs/ft/coder/specialized-subagents.md`
  (YAML def format — update for `usageNotes`),
  `docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md` (Models & Agents PRD).

### Threading order (n3)

agent_def.rs struct + Debug → models.proto (AssistantEntry/Create/Update) → store.rs (column,
NewAssistant, create/update/list, migration) → service.rs handlers → assistant_def.rs projection →
web dialogs + useModelRegistryFanOut → cypress specs → docs.

## Exploration 2 — The subagent turn engine (`packages/tddy-discovery`)

### Turn loop

- `SubagentSession` trait (subagent.rs:139-174): `take_turn(&mut self, TurnRequest) ->
  PromptOutcome`, `prompt`.
- `take_turn` impl (subagent.rs:1723-1754): budget via `request.budget_within(self.max_turns)` →
  `TurnBudget { turns, clamped_to }` (turn_request.rs:111-130); `repeated_calls.forget_earlier_calls()`
  on prompt/correction/rewind; rewind `self.transcript.rewind_to(rewind_point)`; records
  `appended_from = transcript.len()`; pushes user prompt (1742-1744) and **correction as a plain
  `ChatMessage::user` text message** (1745-1748); `run_turn_loop(budget.turns)` then
  `outcome.messages = transcript.descriptors_from(appended_from)`.
- `run_turn_loop` (subagent.rs:1641-1710): loops `run_one_turn`; context-length refusal →
  `context_exhausted_outcome`; budget out → `tools_called.total_outage()` guard or
  `run_synthesis_turn()`.
- `run_one_turn` (subagent.rs:1373-1436): `send_turn_and_check_final_answer` (1012-1107) →
  `TurnStep::FinalAnswer` or `Continue`. On Continue with tool_calls: push assistant message
  (1401-1404), then per call `dispatch_bounded` (1346-1370, bound-tools + `RepeatedCalls::admit` +
  `dispatch_tool_call` 905-978) → **result appended at 1408-1415** via
  `transcript.push_marked(ChatMessage::tool_result(dispatch.tool_result_payload(), call.id, name),
  produced_nothing)`. **The single hook site for summaries and yield conditions.**
- `PromptOutcome` (subagent.rs:82-98): stop_reason, content, usage, messages, clamped_max_turns.

### Tool result JSON shapes (summary sources)

- READ: `{content, truncated, total_lines}` both paths (subagent.rs:570-590 Local window_content;
  tool-engine read_window.rs:40-54). charsRead = content.len().
- GLOB/GREP: `{paths|matches, truncated, total_paths|total_matches}` (capped_results
  subagent.rs:545-562; search_window::result_window tool-engine search_window.rs:29-51).
- WRITE: `{bytes_written}` (lib.rs:334-356). DELETE: `{deleted: true}` (lib.rs:426).
- STR_REPLACE: success `{replaced: true, bytes_written, edited_region, edited_line}`
  (lib.rs:359-410) — **occurrence count computed internally (lib.rs:384-390) but not returned**.
  Managed-only (Local rejects, subagent.rs:414-416).
- SHELL blocking `{stdout, stderr, exit_code}` (lib.rs:543-565), background `{job_id}`;
  AWAIT `{stdout, exit_code, completed}` (lib.rs:640-646).
- READ_LINTS `{lints}` (lib.rs:655-668); SEMANTIC_SEARCH index results (lib.rs:686+).
- Errors: `ToolOutcome::err` → `{error}` envelope, `is_error: true`, parsed by
  `CodebaseAccess::parse_dispatch_result` (subagent.rs:206-221). Rejections:
  `tool_arguments::rejection_payload` (826-828); repeats `repeated.payload()` (829).
- Summaries build at the `push_marked` site (1408), where `ToolDispatch::Ran(value)` JSON is in
  hand before `tool_result_payload()` (821-830) stringifies it.

### TurnRequest + MCP

- `TurnRequest` (turn_request.rs:28-34): `prompt, from_message, correction, max_turns`; builders
  `prompting/resuming/from_message/with_correction/within_turns`; ceiling
  `SUBAGENT_MAX_TURNS_CEILING = 50` (:14), floor `1` (:19).
- `subagent_prompt_tool` (server.rs:1818-1845): sessionId, prompt blocks, graceMs, maxTurns →
  `take_a_turn` (1923-2005) → `DeferredTurn` + `run_turn` (subagent_runtime.rs:606+), grace else
  `pending_turn_json`. `subagent_resume_tool` (1856-1896): fromMessageId / correction,
  present-but-empty refusals. Schemas 2409-2473.

### Transcript

- Ids: `push_marked` mints `MessageId("m{ordinal}")`, counter never reused (163-172, 150-154).
- Rewind `rewind_to` (239-255): unknown id errors (260-271); boundary snapping forward over
  following tool-role messages so a tool_call is never left unanswered (245-252).
- Descriptors (200-224): `MessageDescriptor {id, role, tool, tool_calls, is_error, preview}`
  camelCase serde; `ToolCallDescriptor {name, arguments}` cut to 240; `MESSAGE_PREVIEW_CHARS=240`.
- **No append/replace of caller-supplied tool call + result exists.**

### StopReason plumbing (all sites for a new variant)

- `turn_stop_reason` subagent.rs:1110-1115; `agent_stop_reason` session-agents service.rs:365-373
  (exhaustive); `parse_stop_reason` roster/conversation.rs:460-475 (exhaustive, unknown → error);
  `prompt_outcome_json` subagent_runtime.rs:426-443; `subagent.rs:1608` MaxTokens check; proto
  `stop_reason` string on `AgentConversationChunk` (session_agents.proto:93-113) — string-typed, no
  proto enum; both spellers must agree. Unrelated ACP enums: tddy-acp/*, model-registry
  acp_service.rs:535-543 — do not touch.
- Spelling tests: tddy-daemon-rpc/tests/session_agent_conversation_acceptance.rs,
  tddy-daemon/tests/session_agent_remote_acceptance.rs,
  tddy-tools/tests/session_agent_conversation_client_acceptance.rs, tddy-discovery/tests/*.

### RPC layer

- Client `AgentConversationLink::take_turn` (conversation.rs:125-175) → `turn_call` (185-219):
  `PromptAgentConversationRequest` when prompt set (refuses rewind+prompt, prompt+correction),
  else `ResumeAgentConversationRequest` `{from_message_id, correction, max_turns}`. Streams
  `AgentConversationChunk`; final frame carries `messages` + `clamped_max_turns`.
- `parse_message_descriptor` (conversation.rs:407-435): proto → local descriptor; `tool_calls`
  names-only on the wire (TODO at 422-423 to widen).
- Proto: `AgentConversationChunk` (93-113); `AgentMessageDescriptor` (120-128: id, role, tool,
  tool_calls, is_error, preview); `PromptAgentConversationRequest` (203-216);
  `ResumeAgentConversationRequest` (268-285).
- Server framing: session-agents service.rs `within` (355-361), `agent_turn_frames` (419-434),
  `message_descriptor` (439-454).
- `RemoteAgentSession` (conversation.rs:327-377): `context_tokens`/`tail` return 0/empty (TODOs
  363-374).
