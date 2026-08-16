# Model registry (`model_registry`)

Per-daemon store of model **providers**, the **models** they serve, and **assistants** composed from
them — plus the ACP surface that lets one be chatted with. Served as `models.ModelRegistryService`.

Feature doc: [Models & Agents](../../../docs/ft/web/models-and-agents.md).

## Module layout

| File | Role |
|---|---|
| `store.rs` | `ModelRegistryStore` over `sqlx::SqlitePool` — schema, migration, CRUD, name reservation, id minting, file-permission hardening |
| `error.rs` | `ModelRegistryError` (10 variants) → `Status`, plus provider-detail truncation |
| `provider_client.rs` | `ProviderClient` / `ProviderClientFactory` ports |
| `provider_http.rs` | Shared transport: timeouts, `decode`, `unreachable` |
| `ollama.rs` | `/api/tags`, `/api/show`, `/api/ps`, `/api/generate` |
| `openai_compatible.rs` | `/v1/models`, with per-kind credential style |
| `labels.rs` | Capability → label derivation |
| `assistant_def.rs` | Assistant → `SpecializedAgentDef` projection |
| `service.rs` | `ModelRegistryServiceImpl` — the 12 RPCs |
| `acp_service.rs` | `ModelAcpService` — the model-addressed ACP surface |
| `tool_dispatcher.rs` | `EngineToolDispatcher` — `tddy-acp`'s `ToolDispatcher` port, backed by `tddy_tool_engine` |
| `workspace.rs` | `resolve_chat_workspace` — confining an assistant's `cwd` |

## Storage

`<tddy-data-dir>/models.db`. Follows `tddy-core`'s `session_catalog` precedent: `sqlx` **runtime query
API only** (no `query!` macro, so no compile-time database), WAL, `Normal` synchronous, 5 s busy
timeout, `create_if_missing`, `foreign_keys(true)`.

**File mode.** The database is created `0600` via `OpenOptions::create_new().mode()` *before* sqlx
opens it, so SQLite derives `-wal`/`-shm` from it; all three are re-`chmod`ded after `ensure_schema`,
which also repairs a `0644` database an older build left behind. The **parent is deliberately not
`0700`** — unlike `github_token_store.rs`, which owns a dedicated directory, this lives in the shared
`tddy-data-dir` that session processes under other uids legitimately read.

**Migration** is `ALTER TABLE` guarded by `PRAGMA table_info`, so it is idempotent; a database written
before ownership existed is pinned by a test that builds one with raw sqlx.

**Transactions.** Every check-then-act sequence runs under `BEGIN IMMEDIATE` — base-URL check →
insert, id mint → insert, and the assistant-vs-provider dependency check. Unique violations map to
`AlreadyExists`, not `Storage`.

**Ids are never reused.** A deleted provider id moves to `retired_provider_id`, so a refresh racing a
delete cannot leave the next provider inheriting orphaned model rows.

## Ownership

Every RPC authenticates and the resolved OS user is **kept**, not discarded. Reads are unscoped;
`delete_provider`, `update_assistant`, `delete_assistant` and `credential_for` are owner-only.
`credential_for` refuses a non-owner **whether or not a key is stored** — a presence-dependent rule
would break the day the owner adds one, and "no credential, so talk to their endpoint anonymously" is
the silent fallback this repo forbids. `NULL` owner means unowned and writable, for rows predating the
column.

## Provider clients

`ProviderClientFactory::client_for` returns a `Result` and matches every kind **by name** — there is
no catch-all, because an unclassifiable kind previously fell through to the OpenAI-compatible client
*and was handed the stored credential*.

Credential style is per kind: bearer for OpenAI/Fireworks, `x-api-key` + `anthropic-version` for
Anthropic. Ollama receives the credential too — it needs none itself, but a published Ollama often
sits behind a gateway that does.

Errors are truncated to 400 bytes before being stored or returned; an unbounded provider body in
`enumeration_error` would be chunk-framed over LiveKit, where a lost frame wedges the call silently.

## ACP surface

`ModelAcpService` is registered in `rpc_entries` beside `ModelRegistryService`, so it rides HTTP
`/rpc` and — when LiveKit is fully configured — the data channel. Both pushes sit inside the
`if let Some(user_resolver)` block.

Per opened session it resolves the target from `NewSessionRequest.model_target` (provider+model, or
assistant), reads the credential as the caller, and constructs a `ProviderAcpAgent`. That agent is
`?Send` (`Rc`/`RefCell`), so each stream gets its own OS thread with a current-thread runtime and a
`LocalSet`; only channels cross. A prompt runs as a `spawn_local` task while the stream keeps reading,
so a `Cancel` frame can be received while the turn it interrupts is still running.

**Workspace confinement.** `resolve_chat_workspace` trims the `cwd`, refuses empty and relative paths,
`canonicalize`s (which is what makes containment meaningful — a symlink is compared by its target),
requires a directory, and tests `starts_with` against each canonicalised root. Roots come from the
same preamble as `bsp_session_resolver`: the caller's sessions base plus every `main_repo_path` and
`host_repo_paths` entry in their own `projects.yaml`. Resolution happens **only** when the assistant
has tools; `EngineToolDispatcher` additionally refuses any tool not in the assistant's assigned list,
so a tool-less chat cannot reach the engine.

⚠️ The engine runs **in the daemon process, as the daemon uid** — unlike every other `execute_tool`
caller, which runs in a session process or the sandbox under the caller's uid. Confinement here is
path-based only.

## Assistants as agents

`registry_agent_defs` projects each assistant onto a `SpecializedAgentDef`;
`ConnectionServiceImpl::resolvable_agent_defs` merges builtins, `<tddyhome>/agents/*.yaml` and the
registry, registry winning a name clash. `StartSession` resolves `req.agent` against that set before
consulting `allowed_agents`.

`agent_def_for_spawn` attaches the provider credential; `resolvable_agent_defs` — the `ListAgents`
path, answered for every operator — does not. The def reaches the child as `--agent-def-path <file>`,
written `0600` and chowned to the target account, failing the spawn rather than loosening permissions
if that chown is not possible.

`reject_taken_name` refuses empty/whitespace-padded names, coding-backend ids, builtin defs, and this
daemon's `allowed_agents` ids. Reserved-name refusals return `InvalidName`; only genuine duplicate
rows return `AlreadyExists`.

## Tests

`model_registry_store_unit` (55), `ollama_provider_client_integration` (22),
`model_registry_service_acceptance` (21), `model_acp_service_acceptance` (17),
`model_registry_reserved_names_unit` (8), `registry_assistant_as_agent_acceptance` (6),
`agent_def_spawn_argv_unit` (4). Provider HTTP is exercised against
`tddy_testing_commons::stub_http_routed`, which routes by path, records request headers and bodies,
replies in sequence, and `500`s past the end of a script so an unexpected extra round trip fails
loudly.

Known gaps are listed in [docs/dev/TODO.md](../../../docs/dev/TODO.md) under *Models & Agents — open
items at wrap*; the most notable is that the `rpc_entries` registration itself is untested — deleting
either push leaves the suite green while the screen goes dead.
