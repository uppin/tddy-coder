# 2026-08-16 — Models & Agents — open items at wrap

**Category:** Future enhancement
**Source:** models-and-assistants changeset, 2026-08-16

- **A missing provider credential is not a distinct error.** `credential_for` returns
  `Option<String>` and the client simply omits `Authorization`, so a provider that needs a key but
  has none fails as a generic `Provider(...)` 401 carrying the provider's words rather than ours. Add
  a `MissingCredential` variant (`packages/tddy-daemon/src/model_registry/error.rs:10-36`), refused
  before the round trip, with its own web surface. This is the one PRD requirement that shipped
  partial.
- **No `UpdateProvider` RPC.** A provider's key or base URL can only be changed by deleting and
  recreating it — and because `retired_provider_id` never reuses an id, the recreated row is a
  different provider. Assistants must be rebuilt against it.
- **Service registration is untested.** `packages/tddy-daemon/src/main.rs:622-653` — deleting either
  `rpc_entries.push` leaves all 6750 tests green while the Models screen goes dead. Note also that
  the LiveKit binding is conditional on all four of `livekit.url`/`api_key`/`api_secret`/`common_room`
  being set, and both pushes sit inside the `if let Some(user_resolver)` block, so a daemon without a
  user resolver registers neither.
- **Ownership refusal is under-tested.** `LoadModel`, `UnloadModel` and the ACP chat path all resolve
  the credential through the owner check, but only `DeleteProvider` and `RefreshProviderModels` are
  pinned (`model_registry_service_acceptance.rs:668,705`).
- **No service-level test for `UpdateAssistant` / `DeleteAssistant`** (store- and web-level only), no
  `ListAgents` RPC test with a registry wired (only the `agent_list_mapping` mapper), and no
  service-level `LoadModel`-on-cloud `FAILED_PRECONDITION` assertion (only `UnloadModel`).
- **Per-daemon scoping has no Rust test.** It is structural — a per-daemon DB plus a
  `daemon_instance_id` stamp, correctly with no query filter — but two daemons with two DBs are
  exercised only web-side (`ModelsCrossHostAcceptance.cy.tsx:301`).
- **`ProviderAcpAgent::initialize` / `authenticate` are dead in production** — the daemon hand-rolls
  the same reply at `acp_service.rs:546-561`, so the two can diverge silently.
- **`ModelRegistryStore::replace_models` is `pub` with no production caller** (`store.rs:365-374`);
  `record_refresh` is the real path.
- **`ModelSessionTarget.session_token` is unasserted.** The transport auth gate rewrites only
  *top-level* `sessionToken` fields, so the nested one in a stream frame is never touched by it.
  Production sets it (`ModelChatDialog.tsx:42-47`); nothing pins it.
- **A token refresh mid-conversation restarts the chat stream.** `ModelChatDialog`'s memo has
  `sessionToken` in its dependencies, so a refresh rebuilds the session and loses the transcript.
- **Assistant tools run as the daemon uid.** Confinement is path-based — the ACP `cwd` is
  canonicalised and must resolve inside the caller's own sessions base or their own `projects.yaml`
  repo paths — but unlike every other `execute_tool` caller this runs in-process, not in a session
  process or the sandbox under the caller's uid. Uid separation is the open half.
- **OpenAI models offer no Chat.** `/v1/models` reports no capability information, so they carry no
  `llm` label. Fireworks reports per-model flags and Ollama derives from `/api/show`; closing OpenAI
  needs a different endpoint or an operator-set flag on the model row.
