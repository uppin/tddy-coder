# PRD: Models & Agents — provider registry, model lifecycle, and assistants

**Created:** 2026-08-16
**Product Area:** Web (spans `tddy-daemon`, `tddy-core`, `tddy-acp`, `tddy-discovery`, `tddy-service`, `tddy-web`)
**Status:** WIP

## Summary

A new **Models & Agents** entry in the `tddy-web` navigation menu, showing every **model** offered by
every **provider** configured on every connected daemon — with its owning daemon, its load state, and
its capability labels — and letting the operator load/unload it, chat with it over ACP, and compose it
with tools into a named, reusable **Assistant**.

## Background

Today the web has no view of what models exist anywhere. `ListAgentModels`
(see [tool-session-model-selection.md](../tool-session-model-selection.md)) enumerates models *per coding
backend*, on demand, inside the session-creation form — it is a dropdown-filler, not a catalog:

- It answers "which `--model` can `cursor` take?", not "what is running on this fleet?".
- It has **no peer forwarding** (`connection_service.rs:6247` — `daemon_instance_id` only keys the
  cache), so it cannot answer anything cross-daemon.
- It has no notion of a model being *loaded*, of a model being an *embedding* model, or of a
  *provider* at all.

Meanwhile Ollama is already used in production paths — the `fastcontext` role in
`sandbox-config.example.yaml` points at `http://localhost:11434`, and `fastcontext-tools-32k.Modelfile`
exists precisely because Ollama's `/v1` endpoint cannot set context length per request. But that
configuration is a hand-edited YAML file on each host. There is no way to see which models a host has
pulled, whether one is resident in VRAM, or to unload one that is squatting on a GPU.

Separately, `SpecializedAgentDef` (`tddy-discovery/src/agent_def.rs:58`) already describes exactly the
thing an operator wants to create — `{ name, label, model, base_url, system_prompt, tools, replaces }`
— and `create_backend` (`tddy-coder/src/run.rs:2550`) already resolves any named def as a selectable
`--agent`. The only reason an operator cannot create one is that the sole sources of defs are
`builtin_agent_defs()` and a YAML file.

This feature turns all of that into a first-class, persisted, cross-daemon surface.

## Terminology

The word "agent" is already taken in this codebase (`ListAgents` → `claude`, `cursor`, `codex-acp`, …
— a *coding backend*). To avoid a collision, this feature introduces three distinct terms:

| Term | Meaning | Persisted as |
|---|---|---|
| **Provider** | An endpoint that serves models: an Ollama instance, an OpenAI account, Fireworks, Anthropic. Has a kind, a base URL, and optional credentials. Belongs to exactly one daemon. | `provider` table |
| **Model** | One model offered by one provider (`qwen3:32b`, `nomic-embed-text`, `gpt-5.2`). Carries capability labels and, for local providers, a load state. | `model` table (cache of provider enumeration) |
| **Assistant** | A named composition of a model + a system prompt + a set of tools. Persisted, then projected into a `SpecializedAgentDef` so it is selectable as `--agent <name>`. | `assistant` table |

The **menu entry is labelled "Models & Agents"** (the operator-facing name), but every type, RPC,
table and test id uses `Assistant`. `ListAgents`/`AgentInfo`/`ListAgentModels` are **not** renamed and
not changed.

## Requirements

### Functional Requirements

#### Provider registry (per-daemon SQLite)

- [x] Each daemon owns a SQLite database of providers, models and assistants, at
      `<tddy-data-dir>/models.db`. (Originally specified as `<auth-storage-dir>/models.db`; changed
      during implementation because `auth_storage` is an `Option<PathBuf>` that may be unset, while
      `tddy_data_dir` is always resolved. See the credential risk below — the data dir is not
      guaranteed 0700 the way the auth-storage dir is.)
- [ ] A provider is **added through the UI**, never auto-detected: kind (`ollama`, `openai`,
      `fireworks`, `anthropic`), display label, base URL, optional API key. Nothing about a
      provider is inferred from the environment.
- [x] The API key is stored in the daemon's SQLite (file mode 0600 — as are its `-wal`/`-shm`
      siblings, which is where a just-written key lives first) and is **never returned** by any
      read RPC — responses carry only a `has_credential` boolean.
- [ ] A provider can be removed. Removing a provider removes its cached models and refuses if any
      assistant still references it (explicit error, not a cascade).
- [ ] Provider rows are scoped to the daemon that holds them; a provider added on `workstation` does
      not appear under `laptop`.
- [x] **Global reads, owner-only writes.** Every operator sees the fleet's providers, models and
      assistants, so the screen is a true overview. Only the row's creator may update or delete it,
      and `credential_for` resolves a key only for its owner — so one operator's API key is never
      usable, readable or destroyable by another. Because refresh, load, unload and chat all
      resolve the credential first, those four are refused (`PERMISSION_DENIED`) against another
      operator's provider; rows written before ownership existed carry no owner and stay writable
      by anyone. See the changeset for the migration choice.

#### Model catalog

- [ ] `RefreshProviderModels` enumerates a provider's models from the provider itself
      (Ollama `GET /api/tags`; OpenAI-compatible `GET /v1/models`) and upserts them into the cache.
      A failed enumeration surfaces as an RPC error — no cached-list fallback, no partial success
      presented as success.
- [ ] Each model carries **capability labels** derived from the provider's own metadata:
      `llm`, `embedding`, `vision`, `tools`, `reranker`. For Ollama the source is the `families` /
      `capabilities` fields of `GET /api/show`; a model whose capabilities cannot be determined is
      labelled `unknown` rather than being guessed as `llm`.
- [ ] Each model carries its **load state**: `loaded` (resident, with the expiry Ollama reports),
      `not_loaded`, or `unsupported` (cloud providers — a remote model has no local residency).
- [ ] The screen lists models from **every connected daemon**, each row showing its owning daemon.

#### Model lifecycle

- [ ] **Load** a model: for Ollama, a zero-token generate with `keep_alive` set, which makes the model
      resident. Reported as `loaded` on the next status read.
- [ ] **Unload** a model: for Ollama, a generate with `keep_alive: 0`, evicting it from VRAM.
- [ ] Load/unload is routed to the **owning** daemon, not the selected one.
- [ ] Load/unload on a cloud-provider model is rejected with a typed error (`unsupported for provider
      kind`), never silently ignored.

#### ACP chat

- [ ] A new **ACP adapter** in `tddy-acp` fronts a provider and speaks the ACP agent side
      (`initialize` / `new_session` / `prompt` / `cancel`), translating to the provider's HTTP API.
      Ollama is reached through the existing OpenAI-compatible client (`tddy-discovery::OpenAiClient`).
- [ ] The operator can open a chat with **any LLM-labelled model** or with **any assistant**, from the
      Models & Agents screen. The chat rides the existing `acp.AcpService` bidi stream and the existing
      `useAcpSession` web client — no second chat implementation.
- [ ] Chatting with a model that is not loaded loads it as part of session start.
- [ ] An assistant's chat has its assigned tools available; tool calls surface as ACP `tool_call` /
      `tool_call_update` session updates, dispatched through `tddy_tool_engine::execute_tool`.

#### Assistants

- [ ] An assistant is created from a model by giving it a name, an optional label, an optional system
      prompt, and a selection of tools. The tool list the picker renders comes from the daemon, not
      from a web constant.
- [ ] `ListAssignableTools` returns the **full exec catalog**, `Shell` included, gated only by a valid
      session token. See "Accepted risks" — this is a deliberate decision, not an oversight.
- [ ] The assistant name must be unique on that daemon and must not collide with a builtin agent id
      (`claude`, `cursor`, `codex`, `stub`, `fastcontext`, …). Collisions are rejected explicitly.
- [ ] An assistant is persisted as a `SpecializedAgentDef` projection, and the daemon's def sources
      grow from `{builtin, yaml}` to `{builtin, yaml, sqlite}`, so a created assistant is
      **immediately selectable as `--agent <name>`** when starting a session.
- [ ] Assistants can be listed, edited and deleted.

#### Screen

- [ ] A **Models & Agents** entry in `DaemonNavMenu`, routed at `#/models`.
- [ ] Three sections: **Providers**, **Models**, **Assistants**. The models table shows name, daemon,
      provider, labels, load state, and per-row Load/Unload/Chat actions.
- [ ] A daemon that is unreachable degrades to an error row for that daemon; the rest of the table
      still renders.

### Non-Functional Requirements

- [ ] **No fallbacks.** A failed provider probe, a failed peer, an unsupported operation and a missing
      credential each surface as a distinct, visible error. Nothing degrades silently into a
      plausible-looking success. (Repo rule; also the established precedent for `ListAgentModels`.)
- [ ] **Credentials never leave the daemon.** No RPC response carries an API key. The create/update
      RPC carries it once, inbound only.
- [ ] **Cross-daemon reads are web-side fan-out**, matching `ListSessions` in the sessions drawer: one
      LiveKit client per common-room daemon, merged in the web. One daemon being down costs one
      section, not the page. No new daemon-to-daemon forwarding is introduced.
- [ ] The SQLite store follows the `session_catalog` precedent: `sqlx` runtime query API (no `query!`
      macro, so no compile-time DB), WAL journal, 5 s busy timeout, `create_if_missing`.
- [ ] Model enumeration is on demand (an explicit Refresh), not a background poller. Load-state reads
      are cheap (`/api/ps`) and may be polled while the screen is open.

## Acceptance Criteria

- [ ] **AC1** — The navigation menu contains a **Models & Agents** entry that navigates to `#/models`.
- [ ] **AC2** — The models table lists models from two connected daemons in one table, each row showing
      its owning daemon.
- [ ] **AC3** — A model's capability labels render from the provider's metadata; an embedding model is
      labelled `embedding` and is **not** offered a Chat action.
- [ ] **AC4** — A loaded model renders as loaded and offers Unload; a not-loaded model offers Load.
- [ ] **AC5** — Clicking Load on a model owned by a non-selected daemon sends `LoadModel` to **that**
      daemon, not to the selected one.
- [ ] **AC6** — Adding a provider through the form persists it and it appears in the provider list;
      the API key is not present in any response the web receives.
- [ ] **AC7** — A provider whose enumeration fails renders an inline error for that provider and no
      models; no stale or invented model list is shown.
- [ ] **AC8** — Creating an assistant from a model with a selected tool set persists it, and it appears
      in the assistants list with those tools.
- [ ] **AC9** — An assistant created on a daemon is returned by that daemon's `ListAgents` allowlist as
      a selectable agent.
- [ ] **AC10** — Opening Chat on a model starts an ACP session against that model and streams the
      agent's reply into the chat pane.
- [ ] **AC11** — A cloud-provider model reads as residency-`unsupported` and is offered neither Load
      nor Unload; a `LoadModel`/`UnloadModel` RPC against one is rejected with `FAILED_PRECONDITION`.
- [ ] **AC12** — A daemon that is unreachable renders an error row for that daemon while the other
      daemon's models still render.

## Design decisions

### Providers/Models/Assistants, not "Agents"

Reusing `Agent` for the new concept would require renaming the existing backend concept across
`connection.proto`, `tddy-core`, `tddy-daemon`, `tddy-web` and the Telegram keyboards. The three new
terms are unambiguous, cost nothing, and leave `ListAgents`/`ListAgentModels` untouched. The **menu
label** stays operator-friendly ("Models & Agents"); the **code** says Assistant.

### Start/stop means load/unload, not process management

An operator's actual problem is a model squatting on VRAM, not a dead Ollama service — and managing
the provider process would mean the daemon shelling systemd as root on each host. Load/unload maps
cleanly onto `keep_alive`, needs no privilege, and gives cloud providers a coherent `unsupported`
answer rather than a meaningless one.

### Per-daemon SQLite, web-side merge

Providers describe local reality (a GPU, a machine's credentials), so the DB belongs to the daemon
that uses it. Merging in the web reuses the exact pattern the sessions drawer already proves
(`crossHostSessions.ts`, `useDaemonClientFor`, `mountWithPerDaemonLiveKitRpc`), keeps failure isolated
per daemon, and avoids adding to the daemon-to-daemon forwarding surface. The cost — assistants are
not shared between hosts — is accepted for this changeset and noted below.

### An Assistant is a persisted `SpecializedAgentDef`

`SpecializedAgentDef` already carries model + base_url + system_prompt + tools + replaces, and
`create_backend` already resolves any named def as `--agent`. Adding SQLite as a third def source is a
few lines; inventing a parallel type would duplicate the schema and still need a mapper. The
assistant becomes usable in real sessions the moment it is created.

### The exec catalog is the tool universe — which requires widening `SubagentTool`

`SpecializedAgentDef.tools` is `Vec<SubagentTool>`, a 6-value enum (Read, Glob, Grep, Write,
StrReplace, Delete). The exec catalog is 10 (adding Shell, Await, ReadLints, SemanticSearch). Rather
than keep two tool vocabularies, `SubagentTool` is **widened to the full `tool_catalog()` set**, so one
name space covers both `tools` and `replaces`. This is additive to the serde contract — existing YAML
keeps deserialising — but it does mean `SubagentTool::is_mutating()` and the `CodebaseAccess` dispatch
gain four arms, and `Shell` in particular is only permitted under `CodebaseAccess::Managed`.

### ACP adapter rather than a bespoke chat RPC

An ACP adapter reuses `acp.AcpService`, `useAcpSession`, the transcript/replay machinery and the
tool-call rendering that already exist. A bespoke `ChatWithModel` streaming RPC would be faster to
land and would then need all of that rebuilt for assistants-with-tools.

## Out of scope

- Sharing or replicating assistant definitions between daemons.
- Pulling/deleting models on the provider (`ollama pull`, `ollama rm`).
- Cost, rate-limit or quota reporting for cloud providers.
- Managing the Ollama service process itself.
- Migrating `ListAgentModels` or the session-creation model dropdown onto this registry.

## Accepted risks

### Any authenticated web user can grant `Shell`

`ListAssignableTools` returns all ten exec-catalog tools; `CreateAssistantDialog` renders `Shell` as a
checkbox; the only gate is a valid session token. An operator who can reach the UI can therefore
create an assistant that runs arbitrary commands on the daemon host.

This was raised explicitly and **accepted**: anyone who can reach this UI can already start sessions
that execute commands on the same host, so `Shell` on an assistant grants no capability they did not
already have. The decision is recorded here so a future reader does not mistake it for a gap. If the
trust model ever changes — a shared or semi-public daemon — the natural gate is an `assignable_tools:`
allowlist in `daemon.yaml`, mirroring the existing `allowed_agents` / `allowed_tools`.

## Known risks

- **Plaintext credentials at rest.** API keys live in the daemon's SQLite at
  `<tddy-data-dir>/models.db`, protected by file mode, not by encryption. A host backup captures
  them. This is now a *sharper* risk than originally written: the DB moved out of the 0700
  auth-storage directory into the data dir, which carries no such guarantee. Accepted for this
  changeset; an env-var-reference mode is the natural follow-up (the schema keeps a nullable
  `credential_ref` column for it). `models.db` and its `-wal`/`-shm` siblings are created 0600 (and
  an existing 0644 one is repaired on the next start). The **parent** is left as it is, unlike
  `github_token_store.rs`'s 0700 auth-storage directory: `models.db` shares `tddy-data-dir` with
  paths session processes read as other uids, so tightening it would break them — and a 0600 file
  in a 0755 directory is already unreadable by those accounts. Only the file *name* is visible.
- **Common room is a trusted peer group.** Per `livekit_peer_discovery.rs:15-22`, any participant able
  to join the room is treated as an eligible daemon. This feature adds provider/model reads and
  load/unload to what such a peer is exposed to. No new trust boundary is created, but the surface
  grows.
- **`Shell` as an assignable assistant tool** is a meaningful privilege. It is gated to
  `CodebaseAccess::Managed` (host-side path confinement) exactly as the mutation tools already are.

## Related documentation

- [Tool-session model selection](../tool-session-model-selection.md) — the existing, different
  `ListAgentModels` path, deliberately unchanged
- [Daemon selector + LiveKit-only RPC routing](../daemon-selector-livekit-rpc.md) — the daemon list and
  RPC-routing model this screen is built on
- [App shell](../app-shell.md) — the navigation menu this adds an entry to
- [Session drawer § Cross-Host Active Sessions](../session-drawer.md) — the web-side fan-out precedent
- [ACP protobuf RPC](../../coder/acp-protobuf-rpc.md) — the `acp.AcpService` stream the chat reuses
