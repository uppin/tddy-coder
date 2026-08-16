# Models & Agents — provider registry, model lifecycle, and assistants

**Product area:** Web (spans `tddy-daemon`, `tddy-acp`, `tddy-discovery`, `tddy-service`, `tddy-coder`)
**Status:** Implemented

## Summary

A **Models & Agents** entry in the `tddy-web` navigation menu (`#/models`) showing every **model**
offered by every **provider** configured on every connected daemon — with its owning daemon, its
capability labels and its residency — and letting the operator load/unload it, chat with it over ACP,
and compose it with tools into a named, reusable **Assistant** that is selectable as `--agent <name>`
when starting a session.

## Terminology

"Agent" was already taken in this codebase (`ListAgents` → `claude`, `cursor`, `codex-acp`, … — a
*coding backend*), so this feature introduces three distinct terms:

| Term | Meaning | Persisted as |
|---|---|---|
| **Provider** | An endpoint serving models: an Ollama instance, an OpenAI account, Fireworks, Anthropic. Kind, base URL, optional credential. Belongs to exactly one daemon. | `provider` table |
| **Model** | One model offered by one provider (`qwen3:32b`, `nomic-embed-text`). Capability labels and, for local providers, a load state. | `model` table (a cache of provider enumeration) |
| **Assistant** | A named composition of a model + system prompt + tools. Projected into a `SpecializedAgentDef`, so it is selectable as `--agent <name>`. | `assistant` table |

The **menu entry** reads "Models & Agents"; every type, RPC, table and test id says `Assistant`.
`ListAgents` / `AgentInfo` / `ListAgentModels` are unchanged — the latter answers a different question
(which `--model` values a coding backend accepts) by a different means, and still does.

## The registry

Each daemon owns `<tddy-data-dir>/models.db`, served by `models.ModelRegistryService`. The store
follows the `session_catalog` precedent: `sqlx` runtime query API (no `query!` macro, so no
compile-time database), WAL journal, `Normal` synchronous, 5 s busy timeout, `create_if_missing`,
foreign keys on.

- **Providers are added explicitly through the UI**, never auto-detected. Nothing about a provider is
  inferred from the environment.
- **`base_url` is validated** on create: http/https only, host required, no embedded userinfo —
  without which an authenticated caller had a daemon-side SSRF whose response body was echoed back.
- **Credentials never appear on a read path.** Every listing query selects
  `credential IS NOT NULL AS has_credential`; only `credential_for` reads the value, and only for the
  row's owner.
- **Everyone reads, the owner writes.** Any operator sees the fleet's providers, models and
  assistants — the screen is a true overview. Only the row's creator may update or delete it, and
  only they can resolve its credential, so one operator's key is never usable by another. Rows
  written before ownership existed carry `NULL` and are writable by anyone; back-filling would have
  been a guess about who configured them, and treating `NULL` as immutable would have turned an
  upgrade into an outage.
- **Provider ids are never reused.** A deleted id moves to `retired_provider_id`, so a refresh racing
  a delete cannot leave the next provider inheriting a stale catalog.
- **Deleting a provider refuses while an assistant references it** — an explicit error, not a cascade.

### Model enumeration

`RefreshProviderModels` re-enumerates from the provider itself and replaces the cache in one
transaction. A failed enumeration is an error: the cache is left untouched, the failure is recorded
against the provider, and no partial result is presented as success. The provider's own message is
returned — truncated to 400 bytes, because an HTML error page in that column would otherwise be
chunk-framed over LiveKit, where one lost frame wedges the call silently.

Provider requests carry a 5 s connect / 30 s request budget, with a 120 s ceiling on a whole
enumeration.

### Capability labels — what Chat is offered for

A label is only ever derived from what the provider actually reports. A model whose capabilities
cannot be determined is labelled `unknown`, never guessed as `llm`, and Chat requires a **positive**
`llm` label:

| Provider | Reports capabilities? | Chat offered |
|---|---|---|
| **Ollama** | yes — `capabilities` from `POST /api/show` | yes, for `completion`-capable models |
| **Fireworks** | yes — `supports_chat` / `supports_tools` / `supports_image_input` per model | yes |
| **OpenAI** | **no** — a `/v1/models` entry is `{id, object, created, owned_by}` and nothing more | **no** |
| **Anthropic** | no; chat is refused up front anyway (the agent speaks OpenAI-compatible completions) | no |

`gpt-4o` and `text-embedding-3-small` are indistinguishable in OpenAI's listing except by name, and
inferring capability from an id prefix is exactly the guess this design exists to prevent.
`supports_chat: false` is likewise read as "nothing here says what this is", never as a negative label.

### Residency

**Load** is a zero-token Ollama generate with `keep_alive: "10m"`; **unload** is the same with
`keep_alive: 0`. Residency is read from `/api/ps`. Cloud models report `unsupported` and their
load/unload is refused with `FAILED_PRECONDITION` **without issuing any request**. Ollama reports an
expiry alongside residency; it is not read or stored.

## Chatting with a model or an assistant

The daemon serves a model-addressed `acp.AcpService` (`ModelAcpService`) alongside the registry,
constructing a `ProviderAcpAgent` per session. It reuses the existing ACP stream and the existing
`useAcpSession` web client — there is no second chat transport.

**Target identity** travels as an optional `ModelSessionTarget` on `NewSessionRequest`, carrying the
session token plus either `provider_id`+`model_id` or `assistant_id`. Not overloaded onto `cwd` (which
this feature needs for its own meaning), and not a separate handshake RPC (a second round trip and a
second piece of expiring state). Being `optional` leaves the session-hosted `TddyAcpService`
byte-for-byte unchanged; the daemon-hosted surface **refuses** a `new_session` without a target rather
than guessing a model.

**Where an assistant's tools run.** An assistant with tools needs a workspace. The ACP `cwd` is
canonicalised — so a symlink is judged by its target — and must resolve inside one of the caller's
**own** roots: their sessions base, and the `main_repo_path` / `host_repo_paths` of their own
`projects.yaml`. Empty, relative and escaping paths are refused, and a tool the assistant was not
assigned is refused, so a tool-less chat cannot reach the engine at all.

The web offers that choice as a project picker sourced from `ListProjects` with `local_only: true`,
read off the assistant's owning daemon — the same file through the same resolver the daemon confines
against, so every path offered is one the daemon will accept. `local_only` matters: a fanned-out list
would include peers' paths, which exist on other hosts and would be refused here.

A hung provider cannot wedge the stream: the chat path carries the same transport budget as
enumeration, and `cancel` is implemented against an in-flight signal that genuinely drops the
outstanding request. Transport errors on the inbound stream send an `AcpError` frame rather than
ending quietly, and seven distinct ACP error codes carry `PermissionDenied` / `NotFound` /
`UnsupportedOperation` / `ProviderUnavailable` through to the pane.

## Assistants as agents

An assistant is created from a model with a name, optional label, optional system prompt (bounded at
8 KiB), and a selection of tools from the **exec catalog** — the ten tools
`tddy_tool_engine::tool_catalog()` publishes, rendered from what the daemon returns rather than a web
constant. The name must be unique on that daemon and must not collide with a builtin agent id, a
coding-backend id, or a `<tddyhome>/agents` def; an empty or whitespace-padded name is refused.

The registry is a **third `SpecializedAgentDef` source** alongside builtins and
`<tddyhome>/agents/*.yaml`, so an assistant created in the UI resolves and starts. `StartSession`
accepts it, and `create_backend` no longer falls through to Claude for an unrecognised agent — an
unknown id is an error naming the known agents.

The resolved def reaches the spawned child as **`--agent-def-path <file>`**, written `0600` and
chowned to the target account. It does not travel on argv: `/proc/<pid>/cmdline` is world-readable for
the life of the session, so a provider credential there would be a leak — and moving it off argv also
removed a latent `E2BIG` from an unbounded system prompt. The credential is attached **only** on the
spawn path; `ListAgents`, answered for every operator, returns keyless defs.

## The screen

`#/models` renders **Providers**, **Models** and **Assistants**, merged across every common-room
daemon by web-side fan-out — one client per daemon, matching what the sessions drawer does for
`ListSessions`. Per-row actions route to the model's **owning** daemon, not the selected one.

Reads degrade **per list**: one daemon's failed assistant read leaves its models visible, and an
unreachable daemon costs one error row rather than the page. "Not connected", "loading", "read failed"
and "genuinely empty" are four distinct states, never one blank panel. Models belonging to a provider
whose last enumeration failed are marked stale, so a stale catalog is not read as current.

## Design decisions

- **Providers/Models/Assistants, not "Agents"** — reusing `Agent` would have meant renaming the
  existing backend concept across proto, core, daemon, web and the Telegram keyboards.
- **Load/unload, not process management** — the operator's problem is a model squatting on VRAM, not a
  dead Ollama service; managing the service would mean the daemon shelling systemd as root.
- **Per-daemon SQLite, merged in the web** — providers describe local reality (a GPU, a machine's
  credentials), and merging client-side keeps failure isolated per daemon.
- **An assistant *is* a persisted `SpecializedAgentDef`** — that type already carried model, base URL,
  system prompt and tools, and `create_backend` already resolved any named def.
- **No fallbacks anywhere** — a failed probe, an unreachable peer, an unsupported operation and a
  refused workspace each surface as a distinct, visible error.

## Accepted risks and known limitations

- **Any authenticated web user can grant `Shell` to an assistant.** `ListAssignableTools` returns the
  full exec catalog, gated only by a valid session token. Accepted deliberately: anyone reaching this
  UI can already start sessions that execute commands on the same host. If the trust model ever
  changes, the gate is an `assignable_tools:` allowlist in `daemon.yaml`, mirroring `allowed_agents`.
- **Assistant tools run in the daemon process, as the daemon uid.** Every other `execute_tool` caller
  runs in a session process or the sandbox under the caller's own uid. What constrains this path is
  the path confinement described above, not uid separation. Uid separation remains open.
- **Provider API keys are plaintext at rest.** `models.db` and its `-wal`/`-shm` siblings are `0600`,
  but the parent is the shared `tddy-data-dir` at `0755` — deliberately readable by session processes
  under other uids — so the filename is visible. No encryption, so a host backup captures every key.
  A nullable `credential_ref` column is reserved for an env-var-reference mode that is not built.
- **The common room is a trusted peer group**, unchanged from the existing multi-host trust model.
- **A provider cannot be edited** — there is no `UpdateProvider`; changing a key or base URL means
  delete and recreate, and since ids are never reused the result is a different provider.
- **A missing credential is not a distinct error** — a provider needing a key but holding none fails
  as a generic provider error carrying the provider's words.

Further open items are tracked in [docs/dev/TODO.md](../../dev/TODO.md) under
*Models & Agents — open items at wrap*.

## Related documentation

- [Tool-session model selection](tool-session-model-selection.md) — the separate, unchanged
  `ListAgentModels` path
- [Daemon selector + LiveKit-only RPC routing](daemon-selector-livekit-rpc.md) — the daemon list and
  RPC routing this screen is built on
- [App shell](app-shell.md) — the navigation menu
- [ACP protobuf RPC](../coder/acp-protobuf-rpc.md) — the `acp.AcpService` stream the chat reuses
