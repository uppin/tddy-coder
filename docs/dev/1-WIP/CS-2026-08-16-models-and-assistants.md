# Changeset: Models & Agents — provider registry, model lifecycle, and assistants

**Created:** 2026-08-16
**Status:** In Progress
**PRD:** docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md

## Affected Packages

- [x] `tddy-service` — new `models.proto` (`ModelRegistryService`); build.rs pass for it
- [x] `tddy-daemon` — SQLite store, provider clients, `ModelRegistryService` impl, registration, SQLite as a third `SpecializedAgentDef` source
- [x] `tddy-acp` — provider-backed ACP agent adapter (the crate's stated extraction target)
- [x] `tddy-discovery` — widen `SubagentTool` to the full exec catalog; `is_mutating` / `CodebaseAccess` arms
- [x] `tddy-tool-engine` — no change (its `tool_catalog()` becomes the assistant tool universe)
- [x] `tddy-web` — `#/models` screen, nav entry, cross-daemon fan-out hook, generated client

## State A (Current)

- **No model catalog anywhere.** `ConnectionService.ListAgentModels`
  (`connection_service.rs:6250`) shells out to `tddy-tools list-models --agent <id>` to fill the
  session-creation dropdown. It is per-*backend*, on demand, cached per `(os_user, daemon, agent)`,
  and has **no peer forwarding** — `daemon_instance_id` only keys the cache.
- **No provider concept.** Ollama is reached generically as an OpenAI-compatible endpoint by
  `tddy-discovery::OpenAiClient` (`openai.rs:286`, POSTs `{base_url}/v1/chat/completions`). The only
  configuration surface is `base_url:` in a hand-edited YAML (`sandbox-config.example.yaml:32-45`,
  `packages/tddy-sandbox-app/src/config.rs:198`).
- **No model load state, no capability labels.** `warmup.rs` probes reachability with a one-token
  completion; nothing reads `/api/ps`, `/api/tags` or `/api/show`.
- **Agent defs come from two places.** `builtin_agent_defs()` (`agent_def.rs:104`, one entry:
  `fastcontext`) and YAML. `create_backend` (`tddy-coder/src/run.rs:2550`) checks
  `specialized_agent_defs` **first**, so any named def is already a selectable `--agent`.
- **`SubagentTool`** (`agent_def.rs:23`) is a 6-value enum; `tddy_tool_engine::tool_catalog()`
  (`catalog.rs:16`) is the 10-tool exec catalog. `replaces` uses the 10-name superset as free strings.
- **`tddy-acp`** is a near-empty extraction target: `mapping.rs` only, 5 pure functions, with a doc
  comment stating "the unified ACP client and the agent implementation land here too".
- **ACP over protobuf works end to end**: `acp.proto` `AcpService.Session` bidi stream,
  `TddyAcpService` (`service_acp.rs:168`), registered by `session_view_adapter_surface`
  (`service.rs:132`), consumed by `useAcpSession` in the web.
- **SQLite precedent exists**: `tddy-core/src/session_catalog/store.rs:15` (`sqlx`, WAL, runtime query
  API, `create_if_missing`) and `tddy-semantic-index` (`rusqlite`).
- **Web** has 7 nav entries (`DaemonNavMenu.tsx:52-121`), a hash-route switch
  (`index.tsx:415-456`), a common-room daemon list (`participantRole.ts:86`), per-daemon clients
  (`useDaemonClientFor`, `selectedDaemon.tsx:319`) and a cross-host merge precedent
  (`utils/crossHostSessions.ts`).

## State B (Target)

- Each daemon owns `<tddy-data-dir>/models.db` holding `provider`, `model` and `assistant` rows.
- A new `models.ModelRegistryService` exposes provider CRUD, model enumeration/refresh, model
  load/unload, the assistant CRUD, and the assignable tool catalog.
- Ollama is a **first-class provider kind** with its own client (`/api/tags`, `/api/show`, `/api/ps`,
  `/api/generate` with `keep_alive`); OpenAI-compatible cloud kinds enumerate via `/v1/models` and
  report `unsupported` for load/unload.
- `tddy-acp` gains a provider-backed ACP agent so a model or an assistant can be chatted with over the
  existing `acp.AcpService` stream.
- `SubagentTool` covers the full exec catalog; `SpecializedAgentDef` sources become
  `{builtin, yaml, sqlite}`, so a UI-created assistant is selectable as `--agent <name>`.
- `tddy-web` has a **Models & Agents** entry at `#/models` rendering Providers / Models / Assistants,
  merged across every common-room daemon, with per-row actions routed to the owning daemon.

## Delta

### New

**`packages/tddy-service/proto/models.proto`** — `service ModelRegistryService`:

```proto
service ModelRegistryService {
  rpc ListProviders(ListProvidersRequest) returns (ListProvidersResponse);
  rpc CreateProvider(CreateProviderRequest) returns (CreateProviderResponse);
  rpc DeleteProvider(DeleteProviderRequest) returns (DeleteProviderResponse);

  rpc ListModels(ListModelsRequest) returns (ListModelsResponse);
  rpc RefreshProviderModels(RefreshProviderModelsRequest) returns (RefreshProviderModelsResponse);
  rpc LoadModel(LoadModelRequest) returns (LoadModelResponse);
  rpc UnloadModel(UnloadModelRequest) returns (UnloadModelResponse);

  rpc ListAssistants(ListAssistantsRequest) returns (ListAssistantsResponse);
  rpc CreateAssistant(CreateAssistantRequest) returns (CreateAssistantResponse);
  rpc UpdateAssistant(UpdateAssistantRequest) returns (UpdateAssistantResponse);
  rpc DeleteAssistant(DeleteAssistantRequest) returns (DeleteAssistantResponse);

  rpc ListAssignableTools(ListAssignableToolsRequest) returns (ListAssignableToolsResponse);
}

enum ProviderKind { PROVIDER_KIND_UNSPECIFIED = 0; OLLAMA = 1; OPENAI = 2; FIREWORKS = 3; ANTHROPIC = 4; }
enum ModelLoadState { MODEL_LOAD_STATE_UNSPECIFIED = 0; LOADED = 1; NOT_LOADED = 2; UNSUPPORTED = 3; }

message ProviderEntry {
  string provider_id = 1;
  ProviderKind kind = 2;
  string label = 3;
  string base_url = 4;
  bool has_credential = 5;      // never the key itself
  string daemon_instance_id = 6;
  string enumeration_error = 7; // non-empty => last refresh failed, surfaced inline
}
message ModelEntry {
  string model_id = 1;          // provider-scoped id, e.g. "qwen3:32b"
  string provider_id = 2;
  string label = 3;
  repeated string labels = 4;   // "llm" | "embedding" | "vision" | "tools" | "reranker" | "unknown"
  ModelLoadState load_state = 5;
  string daemon_instance_id = 6;
  uint64 size_bytes = 7;
}
message AssistantEntry {
  string assistant_id = 1;
  string name = 2;              // the --agent value; unique per daemon
  string label = 3;
  string provider_id = 4;
  string model_id = 5;
  string system_prompt = 6;
  repeated string tools = 7;    // exec-catalog tool names
  string daemon_instance_id = 8;
}
```

**`packages/tddy-daemon/src/model_registry/`**
- `store.rs` — `ModelRegistryStore` over `sqlx::SqlitePool`; `open_pool` + `ensure_schema` mirroring
  `session_catalog/store.rs:15` (WAL, `Normal` sync, 5 s busy timeout, `create_if_missing`, runtime
  query API only). Tables `provider`, `model`, `assistant`; `provider.credential` nullable,
  `provider.credential_ref` nullable and unused (reserved for the env-var mode).
- `provider_client.rs` — `trait ProviderClient { async fn list_models(); async fn load_state(); async fn load(); async fn unload(); }`.
- `ollama.rs` — `OllamaProviderClient`: `GET /api/tags`, `GET /api/show` (capabilities → labels),
  `GET /api/ps` (residency), `POST /api/generate` with `keep_alive` (load / `0` unload).
- `openai_compatible.rs` — `GET /v1/models`; load/unload return the `unsupported` error.
- `labels.rs` — pure `capabilities_to_labels(&OllamaShowResponse) -> Vec<String>`; unknown → `["unknown"]`.
- `service.rs` — `ModelRegistryServiceImpl` implementing the generated trait.
- `assistant_def.rs` — pure `assistant_to_agent_def(&AssistantEntry, &ProviderEntry) -> SpecializedAgentDef`.

**`packages/tddy-acp/src/provider_agent.rs`** — `ProviderAcpAgent` implementing `acp::Agent`:
`initialize` / `new_session` / `prompt` / `cancel` against a provider endpoint via
`tddy_discovery::OpenAiClient`, emitting `AgentMessageChunk` / `ToolCall` / `ToolCallUpdate` session
updates. Tool execution is a **port** — `trait ToolDispatcher { fn tool_defs(); async fn execute(); }`
— so `tddy-acp` never depends on `tddy-tool-engine`; the daemon supplies the engine-backed
implementation. Same shape as `session_catalog`'s `BuildCatalogProvider`, which keeps `tddy-core`
free of `tddy-build`.

**`packages/tddy-testing-commons/src/stub_http_routed.rs`** — a path-routed loopback HTTP stub that
records request bodies and answers `404` for unrouted paths, so an unexpected provider call fails
loudly. Shared by the Ollama and ACP suites.

**`packages/tddy-web/src/components/models/`**
- `ModelsAppPage.tsx` (container, `AppShell variant="scroll" title="Models & Agents"`)
- `ModelsScreen.tsx` (pure presentational)
- `ProvidersPanel.tsx`, `ModelsTable.tsx`, `AssistantsPanel.tsx`, `CreateAssistantDialog.tsx`,
  `AddProviderForm.tsx`, `ModelChatDialog.tsx`
- `useModelRegistryFanOut.ts` (per-daemon clients + merge)
- `src/utils/mergeRegistryEntries.ts` + `mergeRegistryEntries.test.ts` (pure, `bun test`)

### Modified

- `packages/tddy-service/build.rs` — a `models.proto` codegen pass (own `OUT_DIR` subdir, same
  `TddyServiceGenerator` config as the `acp.proto` pass at `:57-64`).
- `packages/tddy-daemon/src/main.rs` (~`:542`) — push `ModelRegistryServiceServer` into `rpc_entries`
  so it rides HTTP `/rpc` and LiveKit alike.
- `packages/tddy-discovery/src/agent_def.rs:23` — widen `SubagentTool` with `Shell`, `Await`,
  `ReadLints`, `SemanticSearch`; extend `is_mutating()` (`:35`).
- `packages/tddy-discovery/src/subagent.rs` — `CodebaseAccess` arms for the four new tools; `Shell`
  permitted only under `Managed`.
- `packages/tddy-daemon/src/agent_list_mapping.rs:12` — include SQLite-backed assistants in
  `agent_allowlist_rows`.
- `packages/tddy-web/src/components/shell/DaemonNavMenu.tsx` — a **Models & Agents** entry
  (`shell-menu-models` → `/models`), placed after **Projects**.
- `packages/tddy-web/src/routing/appRoutes.ts` — `MODELS_ROUTE` + `isModelsPath`.
- `packages/tddy-web/src/index.tsx:415-456` — a `isModelsPath(path) ? <ModelsAppPage />` branch.
- `packages/tddy-web/cypress/support/testIds.ts` — the new test ids.
- `packages/tddy-web/src/gen/models_pb.ts` — regenerated (`bunx buf generate`).
- `packages/tddy-acp/Cargo.toml` — adds `tddy-discovery` (the OpenAI-compatible client), `async-trait`,
  `serde_json`; dev-deps `tddy-testing-commons`, `tokio`. No new external crate enters the workspace
  tree — all are already workspace dependencies.
- `packages/tddy-testing-commons/Cargo.toml` — adds `serde_json` (already workspace-wide) for the
  routed stub's JSON body assertions.

### Removed

Nothing. `ListAgents` / `ListAgentModels` / `AgentInfo` are untouched.

## Milestones

### Milestone 0: Planning

- [x] Create/update PRD documentation
- [x] Create changeset

### Milestone 1: Registry & Models screen

- [x] `models.proto` + codegen pass (Rust + `buf generate` for TS)
- [x] `ModelRegistryStore` (schema, provider/model CRUD) with `sqlx`
- [x] `OllamaProviderClient` (`/api/tags`, `/api/show`, `/api/ps`, `keep_alive` load/unload)
- [x] `OpenAiCompatibleProviderClient` (`/v1/models`; `unsupported` for load/unload)
- [x] `capabilities_to_labels` label derivation
- [x] `ModelRegistryServiceImpl` — providers, models, refresh, load, unload
- [x] Daemon registration in `main.rs`
- [x] Nav entry + `#/models` route + `ModelsAppPage`/`ModelsScreen`
- [x] Cross-daemon fan-out hook + pure merge
- [x] Add-provider form

### Milestone 2: ACP chat

- [x] `ProviderAcpAgent` in `tddy-acp` (initialize / new_session / prompt / cancel)
- [x] Wire it behind the existing `acp.AcpService` stream for a registry-addressed target
- [x] `ModelChatDialog` reusing `useAcpSession`
- [x] Chat action hidden for `embedding`-labelled models
- [x] Start-chat loads an unloaded model

### Milestone 3: Assistants

- [x] `SubagentTool` widened to the exec catalog; `is_mutating` + `CodebaseAccess` arms
- [x] `assistant` table + CRUD RPCs + name-collision validation
- [x] `ListAssignableTools` fed by `tddy_tool_engine::tool_catalog()`
- [x] `assistant_to_agent_def` projection; SQLite as a third def source
- [x] `agent_allowlist_rows` includes assistants
- [x] `AssistantsPanel` + `CreateAssistantDialog`
- [x] Assistant chat has its tools available

## Testing Strategy

### Acceptance Tests

Cypress component specs under `packages/tddy-web/cypress/component/models/`, page object
`cypress/support/pages/modelsScreenPage.ts`, using `mountWithPerDaemonLiveKitRpc` /
`mountWithRecordingLiveKitRpc` + `anInMemoryRpcBackend`.

- [x] **AT1** (AC1) `ModelsNavAcceptance.cy.tsx` — "the navigation menu offers Models & Agents and
      navigates to #/models"
- [x] **AT2** (AC2) `ModelsScreenAcceptance.cy.tsx` — "lists models from both connected daemons in one
      table, each row showing its owning daemon"
- [x] **AT3** (AC3) — "labels an embedding model as embedding and offers it no chat action"
- [x] **AT4** (AC4) — "offers Unload for a loaded model and Load for a not-loaded one"
- [x] **AT5** (AC5) `ModelsCrossHostAcceptance.cy.tsx` — "loading a model owned by a non-selected
      daemon targets that daemon"
- [x] **AT6** (AC6) `ProvidersPanelAcceptance.cy.tsx` — "adds a provider and lists it without echoing
      its api key"
- [x] **AT7** (AC7) — "renders the enumeration error for a failing provider and lists none of its
      models"
- [x] **AT8** (AC8) `AssistantsPanelAcceptance.cy.tsx` — "creates an assistant from a model with the
      selected tools"
- [x] **AT9** (AC9) Rust integration — "an assistant persisted in the registry is listed as a
      selectable agent"
- [x] **AT10** (AC10) `ModelChatAcceptance.cy.tsx` — "opening chat on a model streams the agent reply
      into the chat pane"
- [x] **AT11a** (AC11) `ModelsScreenAcceptance.cy.tsx` — "renders a cloud model as
      residency-unsupported and offers neither Load nor Unload"
- [x] **AT11b** (AC11) Rust integration — "rejects loading a model whose provider has no notion of
      residency"
- [x] **AT12** (AC12) `ModelsCrossHostAcceptance.cy.tsx` — "renders an error row for an unreachable
      daemon while the other daemon's models still render"

### Unit / integration tests

Counts below are the **red-phase** figures, kept for the record. Current counts after the
hardening and wiring passes are in *Test inventory (current)* at the end of this document.

| File | Tests | Pins |
|---|---|---|
| `packages/tddy-discovery/tests/subagent_tool_exec_catalog_red.rs` | 5 | `SubagentTool` covers all ten exec-catalog tools; `from_catalog_name`/`catalog_name` round trip; existing YAML still parses; `Shell` joins the mutating set |
| `packages/tddy-daemon/tests/model_registry_store_unit.rs` | 20 | SQLite CRUD; credential stored but never in a read row; duplicate base URL and duplicate assistant name refused; provider-in-use refused (no cascade); `replace_models` replaces rather than accumulates; `capabilities_to_labels` (incl. `unknown`, never a guessed `llm`); `assistant_to_agent_def` projection |
| `packages/tddy-daemon/tests/model_registry_service_acceptance.rs` | 9 | auth gating; a failed refresh errors and is recorded, never a cached catalog; residency refused with `FAILED_PRECONDITION` (AC11b); `ListAssignableTools` = the exec catalog; builtin-name collision refused; **AC9** — a created assistant joins `agent_allowlist_rows` |
| `packages/tddy-daemon/tests/ollama_provider_client_integration.rs` | 8 | the Ollama wire contract: `/api/tags`, `/api/show` → labels, `/api/ps` → residency, `/api/generate` with `keep_alive: "10m"` / `0`; cloud enumeration via `/v1/models`; cloud load refused without a request |
| `packages/tddy-acp/tests/provider_agent_acceptance.rs` | 8 | ACP handshake advertises the configured model; completion → `AgentMessageChunk` + `EndTurn`; system prompt leads the conversation; provider failure → ACP error, not an empty turn; assigned tools reach the model and dispatched calls surface as `ToolCall` |

Supporting test infrastructure: `tddy-testing-commons::stub_http_routed` — a path-routed loopback
HTTP stub that records request bodies and `404`s anything unrouted, shared by the Ollama and ACP
suites. Its own tests pass (2 at the time of writing, 11 now).

### Test Level Decisions

| Aspect | Level | Rationale |
|---|---|---|
| Nav entry, route, screen rendering, per-row actions | Cypress component | The behavior *is* the DOM; `mountWithRpc` gives a real transport without a daemon |
| Cross-daemon merge + owning-daemon routing | Cypress component (`mountWithPerDaemonLiveKitRpc`) | The only tool that can assert *which* daemon identity an RPC targeted |
| Pure merge / sort / dedupe of registry rows | `bun test` unit | Pure `.ts`, no JSX runtime needed — matches `crossHostSessions.test.ts` |
| Capability-label derivation | Rust unit | Pure function over a provider payload; exhaustive cases cheaply |
| `assistant_to_agent_def` projection | Rust unit | Pure mapping, and the contract that makes AC9 work |
| SQLite store CRUD, uniqueness, referential refusal | Rust integration (tempdir DB) | Real `sqlx` against a real file; a fake would test nothing |
| Provider clients against Ollama's HTTP shapes | Rust integration (local HTTP stub) | Pins the wire contract (`/api/tags`, `/api/ps`, `keep_alive`) without needing Ollama |
| `ModelRegistryServiceImpl` behavior incl. error mapping | Rust integration | Store + fake `ProviderClient`; asserts no-fallback error surfacing |
| `ProviderAcpAgent` session lifecycle | Rust integration | Drives the real `acp::AgentSideConnection`, as `tddy-acp-stub` does |
| Real Ollama end to end | **Not tested** | Requires a GPU host and a pulled model; out of CI's reach |

## Technical Debt

- API keys are plaintext at rest in the daemon's SQLite. The `credential_ref` column is reserved for
  an env-var-reference mode but is unused by this changeset.
- Assistants do not replicate between daemons; one defined on the laptop is invisible on the
  workstation.
- `ListAgentModels` and the session-creation model dropdown continue to use the separate
  `tddy-tools list-models` path. Two model catalogs coexist until a later changeset unifies them.
- `ClaudeAcpBackend::default()` (`tddy-core/src/backend/acp.rs:360`) still hardcodes
  `bunx claude-agent-acp` with no env/config override, unlike `CodexAcpBackend`. Out of scope here,
  logged under Future Enhancements.
- `tddy-core/src/backend/acp.rs` and `codex_acp.rs` remain ~80% duplicated; this changeset adds a third
  ACP implementation in `tddy-acp` rather than unifying them.

## Validation Results (2026-08-16, `/pr-wrap` step 1–2)

> **Status: every item below has since been resolved** — see *Integration wiring* and *Registry
> hardening*. The findings are kept verbatim because they record why the design ended up as it did,
> not because they are outstanding. A second `/pr-wrap` pass (steps 3–4) found four further
> blockers; those are tracked in *Second-pass findings* at the end.

**Verdict: not ready to merge.** Build is clean and all 68 tests pass, but three integration seams
were never wired, and no test covers them because every test pins a piece in isolation.

### Blocking (all fixed — see *Integration wiring* and *Registry hardening*)

- **[CRITICAL] An assistant is listed as an agent but cannot be used as one.**
  `assistant_to_agent_def` has **no production caller** (only its definition, the `mod.rs` re-export
  and a unit test). Agent-def resolution (`connection_service.rs:2562`) still reads only
  `<tddyhome>/agents/*.yaml` + builtins. `StartSession` (`connection_service.rs:5557`) validates
  `req.agent` against `config.allowed_agents()` alone, so a registry assistant is **rejected** when
  the allowlist is non-empty and **silently falls through to `AnyBackend::Claude`**
  (`tddy-coder/src/run.rs:2611`) when it is empty. Milestone 3 is not delivered.
  AC9 passed because it only asserted the `ListAgents` half.
- **[CRITICAL] The model chat cannot work outside the test fake.** `ModelChatDialog` addresses
  `AcpService` at `daemon-{instanceId}`, but the daemon registers **no** `acp.AcpService` in
  `rpc_entries` — ACP is mounted per *session* by `session_view_adapter_surface`. The handshake also
  carries no provider/model identity, and `ProviderAcpAgent` has **no production caller**.
  Milestone 2 is not delivered. `ModelChatAcceptance` is green only because it layers
  `.implement(AcpService, …)` onto the in-memory backend.
- **[CRITICAL] `models.db` is created world-readable (0644) holding plaintext API keys**, at
  `tddy_data_dir/models.db`, with no `UMask=` in the installed units. In system mode sessions run as
  other uids on the same host. `github_token_store.rs:34` already does 0700/0600 correctly.
- **[CRITICAL] `DefaultProviderClients::client_for` falls back to the OpenAI-compatible client for
  unknown provider kinds and sends the stored credential to them** (`service.rs:323`).

### Needs a product decision

- **`Shell` is grantable from a browser form.** `ListAssignableTools` returns the full exec catalog;
  `CreateAssistantDialog` renders `Shell` as a checkbox; the only gate is a valid session token.
  That is web-grantable arbitrary command execution on the daemon host.
- **The registry is daemon-global with no owner column.** Every RPC authenticates and then discards
  the resolved user, so any operator can read/delete another's providers — in a store holding
  everyone's API keys.

### Non-blocking (all fixed — see *Registry hardening*)

Ollama client never receives the credential (silently dropped while `has_credential: true`); no HTTP
timeout on any provider request; `enumeration_error` stores the provider's response body verbatim and
unbounded (>60 KB wedges a LiveKit RPC silently) and `base_url` is unvalidated (daemon-side SSRF with
the body echoed back); provider ids reused after deletion with no FKs → orphaned model rows; three
check-then-act sequences outside transactions; refresh's error path can mask the provider failure with
a SQLite error; `reject_taken_name` checks only `builtin_agent_defs()` and accepts `name: ""`;
`ANTHROPIC` routed through bearer auth (needs `x-api-key`); stale models render unmarked after a failed
enumeration (violates AC7); `isChatCapable` offers Chat for any non-`embedding` label, so every
`"unknown"`-labelled cloud model gets one; fan-out is `Promise.all` so one failed list blanks a whole
daemon; provider-error maps keyed by `providerId` alone collide across daemons; silent no-op on Refresh
when the client is null.

## Integration wiring (resolves the two agent/chat blockers above)

The two `[CRITICAL]` seams about the assistant-as-agent and the model chat are now wired. The
remaining `[CRITICAL]` items (`models.db` file mode, `DefaultProviderClients::client_for`'s
unknown-kind fallback) and the product decisions are untouched by this pass.

### An assistant is usable as an agent, not merely listed

- **`registry_agent_defs(store)`** (`model_registry/assistant_def.rs`) is the production caller of
  `assistant_to_agent_def`: every assistant, paired with its provider row, projected onto a
  `SpecializedAgentDef`. An assistant whose provider row is gone fails the call rather than being
  dropped — omitting it would turn "this endpoint is unknown" into "there is no such agent".
- **`ConnectionServiceImpl::resolvable_agent_defs`** (now the single resolution point behind
  `resolve_specialized_agent_defs`) merges `{builtin, <tddyhome>/agents/*.yaml, sqlite}`, with the
  registry winning a name collision on the same rule that already makes YAML beat a builtin. It is
  `async` because the registry is a SQLite read; the three `start_*_session` call sites `.await` it.
- **`StartSession`** resolves `req.agent` against that same def set first and only falls back to the
  `allowed_agents` check when it is not a def, so a registry assistant is startable with a non-empty
  allowlist.
- **The def travels with the spawn.** A spawned `tddy-coder` resolves `--agent` against the builtins
  and `<tddyhome>/agents` only — it cannot read the daemon's `models.db`. So the daemon passes the
  def it already resolved as **`--agent-def <json>`** (`SpawnOptions.agent_def_json` →
  `SpawnRequest.agent_def_json` → argv), and `resolve_specialized_agent_defs` in the coder merges it
  into the set `create_backend` consumes. A malformed `--agent-def` fails the run rather than being
  dropped. Considered and rejected: teaching `tddy-coder` to read `models.db` itself (a second
  reader of the daemon's schema, and a new `sqlx` dependency edge), and materialising each assistant
  as a YAML file under `<tddyhome>/agents` (two sources of truth to keep in sync on update/delete).
- **`create_backend`'s catch-all is gone.** `"claude"` is now an explicit arm; anything else that is
  neither a coding backend nor a resolved def is an `anyhow` error naming the known agents. The
  callers that relied on the old fall-through all pass `"claude"` explicitly
  (`args.agent.unwrap_or("claude")`, `backend_from_label`'s own `_ => ("claude", …)`), so none lost
  behaviour. `create_backend` now returns `anyhow::Result<SharedBackend>`.
- **`--agent`'s static clap `value_parser` allowlist is removed** (the `AC14` TODO at
  `run.rs:576`): a def's name is only knowable after `--tddy-data-dir`/`--agent-def` are parsed, so
  validation moved to `create_backend`, where the whole def set is known.
- **`reject_taken_name`** now refuses an empty/whitespace-only or whitespace-padded name, the
  coding-backend ids (`tddy_coder::run::BUILTIN_BACKEND_AGENT_IDS`, the single source of truth), the
  builtin defs, and this daemon's `allowed_agents` ids — supplied at open time via
  `ModelRegistryStore::reserving_agent_ids`, since the store cannot read daemon config and the
  create/delete signatures are owned elsewhere.

### The model chat is served by the daemon

- **`ModelAcpService`** (`model_registry/acp_service.rs`) implements `acp.AcpService` and is pushed
  into the daemon's `rpc_entries` next to `ModelRegistryService`, so it rides HTTP `/rpc` and
  LiveKit alike. It builds a `ProviderAcpAgent` per opened session from the registry: assistant (or
  provider+model), base URL, and the credential from `credential_for`.
- **Threading**: `ProviderAcpAgent` holds `Rc`/`RefCell` (the ACP SDK's traits are `?Send`), so each
  stream gets its own OS thread running a current-thread runtime + `LocalSet`; only channels cross
  that boundary.
- **`EngineToolDispatcher`** (`model_registry/tool_dispatcher.rs`) is the daemon's implementation of
  `tddy-acp`'s `ToolDispatcher` port, backed by `tddy_tool_engine::execute_tool` and confined to the
  session's `cwd`. An assistant with tools and no `cwd` is refused at `new_session`.

### Carrying the target through the ACP handshake

`NewSessionRequest` gains an optional **`ModelSessionTarget`** (`acp.proto`), carrying
`session_token`, `provider_id` + `model_id`, or `assistant_id`.

Why there, and why not the alternatives:

- **Not `cwd`.** `cwd` already has a meaning this feature needs — it is the workspace an assistant's
  tools run in. Encoding a `tddy-model://…` URL in it would overload one field with two contracts.
- **Not the `AcpClientMessage` envelope.** The target is a property of the *session*: `initialize`
  is answered before any target is known, and one stream may open a session for a different model
  next time.
- **Not a separate `models.proto` RPC handing out a session handle.** That is a second round trip
  and a second piece of state to expire, for the same information the handshake already carries.

It is `optional`, so the session-hosted `TddyAcpService` — where the agent *is* that session's
workflow and nothing has to be named — is unchanged, and an external ACP client that never sets it
is unaffected. The daemon-hosted surface refuses a `new_session` without one rather than guessing a
model.

`useAcpSessionOverClient` takes an optional `modelTarget` and puts it on the `new_session` frame;
`ModelChatDialog` supplies the row's provider/model plus the auth context's session token.

### Tests added

| File | Tests | Pins |
|---|---|---|
| `packages/tddy-daemon/tests/registry_assistant_as_agent_acceptance.rs` | 4 | the registry is a def source carrying model/base_url/tools/system prompt; `StartSession` accepts an assistant name under a non-empty allowlist; an unknown agent is still refused |
| `packages/tddy-daemon/tests/agent_def_spawn_argv_unit.rs` | 2 | the resolved def reaches the child as `--agent-def`; a self-resolvable agent carries none |
| `packages/tddy-daemon/tests/model_registry_reserved_names_unit.rs` | 7 | the assistant name space: coding backends, `allowed_agents` ids, builtin defs, empty/whitespace names |
| `packages/tddy-daemon/tests/model_acp_service_acceptance.rs` | 6 | the daemon's ACP surface end to end against a stub provider: reply streaming, the target's model, an assistant's tools running in the session workspace, its system prompt leading, an invalid token refused before any provider call, an unknown provider refused |
| `packages/tddy-coder/src/run.rs` (`agent_def_handover_tests`) | 4 | `--agent-def` joins the resolvable set; a malformed one fails the run; an unresolvable agent is an error, not Claude; `claude` still resolves |

**Not covered:** that `main.rs` pushes the `AcpService` entry into `rpc_entries`. The daemon binary's
service wiring has no test harness in this repo (no existing suite asserts on `rpc_entries`), so
this remains the one hand-verified line of the chat path.

## Registry hardening (resolves the remaining criticals and the correctness defects)

The two `[CRITICAL]` items left after the integration pass — the world-readable `models.db` and the
unknown-kind fallback in `DefaultProviderClients` — are fixed, the registry gained an owner column,
and the non-blocking list above is worked through. Everything here is inside
`packages/tddy-daemon/src/model_registry/**`; no proto, no web change.

### Credentials at rest

`models.db` is created `0600` **before** SQLite opens it (`create_new` + `mode`), and the database
plus its `-wal`/`-shm` siblings are re-restricted to `0600` after the schema runs — which also
repairs a database an earlier daemon left at `0644`. WAL matters here: a just-written credential
lives in `models.db-wal` before it lives in the database.

**The parent directory is deliberately *not* set to `0700`,** unlike `github_token_store.rs`. That
store owns a dedicated `auth_storage` directory; `models.db` sits in the shared `tddy-data-dir`,
which session processes running as **other uids** read (`projects/`, the per-user session bases), so
tightening it would break them. A `0600` file inside a `0755` directory is already unreadable by
those accounts — the directory mode would only hide the file *name*. If hiding the name is wanted,
the clean move is a dedicated `<tddy-data-dir>/model-registry/` subdirectory, which is a path
change and so a separate decision.

### Everyone reads, the owner writes

`provider` and `assistant` gained a nullable `owner` column, holding the OS user the caller's
session token resolved to.

- **Reads stay unscoped.** `list_providers`, `list_models`, `list_assistants` and
  `ListAssignableTools` return the whole daemon's rows: the screen is a fleet overview, and an
  operator who cannot see a provider cannot see why a model is missing either.
- **Writes are the owner's.** `delete_provider`, `update_assistant` and `delete_assistant` answer
  `PermissionDenied` to anyone else.
- **`credential_for` resolves a key only for its owner**, and refuses *regardless of whether a key
  is stored*. Two reasons: a rule that depended on the key's presence would let a colleague's
  refresh work today and start failing the day the owner adds one; and a caller told "no
  credential" would go on to talk to someone else's endpoint unauthenticated. The consequence is
  explicit: **refresh, load, unload and chat against another operator's provider are refused**,
  since all four resolve the credential first. That is the "no silent fallback" reading of the
  rule; if the fleet wants shared *use* of a colleague's keyless local Ollama, the follow-up is an
  explicit `shared: true` flag on the provider row, not a hole in `credential_for`.

**Migration choice: a nullable column where `NULL` means "unowned, writable by anyone".** A
database written before this change has its rows migrated in place (`ALTER TABLE … ADD COLUMN`
guarded by `PRAGMA table_info`), and they keep `NULL`. The alternatives were worse:

- *Backfill with the daemon's own user* — a guess about who configured the provider, and on a
  multi-operator host it would hand every existing row to whoever the daemon happens to run as.
- *Backfill with the first caller who touches a row* — the same guess, made later and less visibly.
- *Treat `NULL` as "owned by nobody", i.e. immutable* — a running daemon's existing providers could
  never be deleted or refreshed again, which turns an upgrade into an outage.

`NULL` therefore means exactly what it says: this row predates ownership, and nothing is known
about who owns it. Every row created from now on has an owner.

### Correctness

| Defect | Fix |
|---|---|
| Ollama dropped the credential while `has_credential: true` | `OllamaProviderClient` takes the credential and sends it as `Authorization: Bearer` on every call. Threading rather than refusing a key on `OLLAMA`: Ollama itself needs none, but an Ollama published to a network sits behind a proxy/gateway/hosted tier that does, and refusing would make those deployments unconfigurable. |
| No HTTP timeout anywhere | `ProviderHttp` (connect 5 s, request 30 s, enumeration budget 120 s) builds every provider client. An enumeration is additionally wrapped in the budget, since Ollama's is `/api/tags` + `/api/ps` + one `/api/show` **per model**. |
| Unbounded `enumeration_error` reaching every client | `truncate_provider_detail` (400 bytes) at both provider clients **and** in `record_enumeration_error`, so nothing past a few hundred bytes is stored or returned — a `>60 KB` `ListProviders` is chunk-framed over LiveKit, where a lost frame wedges the call silently. |
| Unvalidated `base_url` (daemon-side SSRF) | `validate_base_url` on create: `http`/`https` only, a host required, and no embedded userinfo (which used to be echoed back inside the unreachable message). |
| Provider ids recycled after deletion; no FKs | Deleted ids are recorded in `retired_provider_id` and never minted again; `write_models` re-checks the provider **inside the write transaction**, so a refresh racing a delete is refused rather than orphaning rows; the `model` and `assistant` tables now declare `FOREIGN KEY (provider_id)` and connections set `foreign_keys=ON`. The in-transaction check is what enforces this on databases created before the constraint existed. |
| Three check-then-act sequences outside a transaction | `create_provider`, `create_assistant` and `delete_provider` each run under `BEGIN IMMEDIATE` (a deferred `BEGIN` only takes the write lock at the first write, so two callers can both pass the same check). A unique-constraint loss is mapped to `AlreadyExists`, not `Storage`. |
| Refresh = two writes; error path masked the cause | `record_refresh` writes the catalog and clears the error in one transaction. On failure, a failed *recording* is logged and the **provider's own error** is still what returns. |
| `ANTHROPIC` routed through bearer auth | Enumeration speaks Anthropic properly (`x-api-key` + `anthropic-version`) via `CredentialStyle`. The **chat** path refuses an Anthropic provider up front, because `ProviderAcpAgent` speaks OpenAI-compatible completions, which Anthropic does not serve — a `TODO` marks the `/v1/messages` follow-up. The enum value is kept (removing it would force a web change), but no path through it silently cannot work. |
| `create_assistant` did not validate the model | An empty/whitespace `model_id` is refused (it would produce an agent def with no model). It is **not** checked against the cached catalog, on purpose: that cache is empty until someone refreshes and stale whenever the host changed, so checking it would refuse legitimate models. The provider row — the authoritative part — is still checked. A typo'd model surfaces as the provider's own error at the first prompt. |
| Raw `sqlx` messages reached the caller | `Storage` renders as a fixed string in both `Display` and `Status`; the sqlx detail (database path, constraint and column names) is logged instead. |
| `apply_residency` synthesized `labels: vec![]` | An uncached model comes back labelled `unknown` (`UNDETERMINABLE_LABEL`), matching the proto's own rule that "we could not tell" is never an empty list. `openai_compatible`'s `unknown` for every cloud model is unchanged and correct. |

### Tests added (registry hardening)

| File | Added | Pins |
|---|---|---|
| `packages/tddy-daemon/tests/model_registry_store_unit.rs` | 27 → 46 | `0600` on the database and its `-wal`/`-shm` (and the repair of a `0644` one); base URL scheme/host/userinfo refusals; a deleted provider's id is never re-minted; caching models for a deleted provider is refused; a fresh catalog and the cleared error land together; the provider's error page is stored bounded; every operator's rows are listed but only the owner may delete/update/read the credential; a pre-ownership database migrates and its rows read as unowned; an assistant with no model id is refused |
| `packages/tddy-daemon/tests/model_registry_service_acceptance.rs` | 12 → 21 | `PermissionDenied` on deleting and refreshing another operator's provider; `ListProviders` is fleet-wide; a refresh reports the provider's failure even when recording it fails (the fake deletes the row mid-enumeration); an uncached model is labelled `unknown`; `DefaultProviderClients` refuses an unset and an unknown kind while still resolving all four known ones; a storage failure leaks neither path nor detail |
| `packages/tddy-daemon/tests/ollama_provider_client_integration.rs` | 13 → 19 | the stored key reaches Ollama as a bearer token (and nothing is invented without one); an Anthropic key is presented as `x-api-key` + `anthropic-version` and never as a bearer; a host that accepts and never answers is given up on; an enumeration that outlasts its budget fails saying so; a 200 KB error page becomes a bounded message |
| `packages/tddy-daemon/tests/model_acp_service_acceptance.rs` | 6 → 8 | a chat against another operator's provider is refused before any provider call; a chat with an Anthropic provider is refused instead of posting to a `/v1/chat/completions` it does not serve |

Every new assertion was mutation-checked: the corresponding production line was reverted to its
previous form and the tests that must fail did (13 store, 5 service, 5 provider-client, 2 ACP).
