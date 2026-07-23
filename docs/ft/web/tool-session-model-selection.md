# Tool-session model selection — pick the backend model when creating a tddy-coder session

**Product area:** Web
**Status:** Implemented

## Summary

The `CreateSessionPane` "New session" form lets the operator pick the **model** the underlying
backend runs with — for **tool** (tddy-coder) sessions as well as **claude-cli** sessions. The
model list for a backend is, wherever possible, **enumerated from the underlying agent command
itself** (e.g. `cursor-agent --list-models`, or the ACP `available_models` a codex-acp/claude-acp
agent advertises), rather than a static web list. Backends whose command exposes no enumeration
(`claude`, `codex` non-ACP) use a **curated list maintained in `tddy-core`**.

Enumeration is fetched **on demand** when the operator picks an agent, via a new
`ListAgentModels` RPC, so the (subprocess-spawning, auth-dependent) probe only runs for the chosen
backend and never slows the form's initial load.

For a tool session the chosen model is threaded through the daemon's spawn path to
`tddy-coder --model`, seeding the session-wide conversation model (`context["model"]`) used by
every backend invoke. `CreateSessionPane` is the only session-creation surface in scope (it backs
both the SessionDrawer "+ New session" flow and the PR-stack "Start session" dialog). The legacy
`ConnectionScreen` inline form is unchanged.

## Model sourcing per backend

| Agent | Source | Mechanism |
|-------|--------|-----------|
| `cursor` | **command** | `cursor-agent --list-models` → parse `id - label` lines; `(current, default)` marks the default |
| `claude-acp` | **command (ACP)** | `SessionModelState.available_models` from the ACP `new_session`/`load_session` response |
| `codex-acp` | **command (ACP)** | same ACP `available_models` |
| `claude` | **curated** | maintained (id,label) list in `tddy-core` (opus / sonnet / haiku) |
| `codex` | **curated** | maintained list in `tddy-core` (gpt-5) |
| `stub` | static | `stub` |
| `claude-cli` (session type) | **curated** | maintained Claude full-id list in `tddy-core` (claude-opus-4-8 / claude-sonnet-4-6 / claude-haiku-4-5-20251001) |

## API Surface

### Proto (`packages/tddy-service/proto/connection.proto`)

```proto
service ConnectionService {
  // ... existing ...
  rpc ListAgentModels(ListAgentModelsRequest) returns (ListAgentModelsResponse);
}

message ListAgentModelsRequest {
  string session_token = 1;
  // Agent id ("claude", "cursor", "codex-acp", …) or the pseudo-agent "claude-cli".
  string agent = 2;
  // Optional daemon instance to run the probe on (multi-host). Empty = local.
  string daemon_instance_id = 3;
}
message ListAgentModelsResponse {
  repeated ModelInfo models = 1;
  // Id (within `models`) to preselect. The backend's current/default model.
  string default_model = 2;
}
message ModelInfo {
  string id = 1;     // value passed as --model (e.g. "gpt-5.2", "opus", "claude-opus-4-8")
  string label = 2;  // human-readable (e.g. "GPT-5.2", "Claude Opus 4.8")
}
```

`StartSessionRequest.model` (field 8) is reused unchanged — previously claude-cli-only, now also
populated for tool sessions. `AgentInfo` / `ListAgents` are **not** changed.

### tddy-core (`packages/tddy-core/src/backend/`)

```rust
pub struct BackendModel { pub id: String, pub label: String }
pub struct BackendModels { pub models: Vec<BackendModel>, pub default_model: String }

// New CodingBackend trait method (async, default impl = curated list for the backend's name):
async fn list_models(&self) -> Result<BackendModels, BackendError>;
```

- `CursorBackend::list_models` — spawns `<binary> --list-models`, parses the text catalog.
- `ClaudeAcpBackend` / `CodexAcpBackend::list_models` — runs an ephemeral ACP `initialize` +
  `new_session` and reads `SessionModelState.available_models` (the handshake already exists; it
  currently discards `.models`).
- `ClaudeCodeBackend` / `CodexBackend::list_models` — return the curated list from a
  `curated_models_for_agent(name)` helper (single source of truth, kept in sync with
  `default_model_for_agent`).
- A free function `claude_cli_models() -> BackendModels` for the claude-cli pseudo-agent.

### tddy-tools subcommand

```
tddy-tools list-models --agent <id> [--cursor-cli-path P] [--codex-acp-cli-path P] …
```

Constructs the backend for `<agent>` (same binary-resolution rules as `tddy-coder`), calls
`list_models()`, prints `{ "models": [{"id","label"}], "default_model": "<id>" }` JSON on stdout.
`--agent claude-cli` returns the curated Claude full-id catalog.

### Daemon

- `ListAgentModels` handler shells out to `tddy-tools list-models --agent <agent>` (via the
  configured tool path / spawn worker), parses the JSON, returns `ListAgentModelsResponse`. Result
  is cached per (agent, daemon, **OS user**) with a short TTL to avoid re-probing on every agent
  toggle — the cache is keyed by OS user because cursor/ACP catalogs are account-specific, so a
  shared (agent, daemon) key would leak one user's catalog to another on a multi-tenant daemon. The
  probe runs `run_capture_as_user` with `current_dir` set to the user's **home** (the daemon cwd may
  be unreadable after setuid, and the ACP probe opens a session against the cwd).
- `StartSession` tool branch threads `req.model` into `SpawnOptions.model`
  (`spawner.rs` + `spawn_worker.rs`); the spawner appends `--model <model>` when non-empty.

### Web (`CreateSessionPane.tsx`)

- On agent change (tool) or when switching to claude-cli, call
  `ListAgentModels({ agent })`; render a Model `<select>` from `models`, preselect `default_model`,
  and show a loading state while the probe runs.
- Send the selected `model` in the tool `startSession` call (currently hardcoded `""`).
- Remove `CreateSessionPane`'s dependency on the hardcoded `CLAUDE_CLI_MODELS` constant; its
  claude-cli dropdown is fed by `ListAgentModels({ agent: "claude-cli" })` (see the known limitation
  below — the constant itself is retained for the out-of-scope legacy `ConnectionScreen`).
- Reuse the existing `create-session-model-select` test id (one model select visible at a time).

## Behavior

- Model list reflects what the selected backend actually supports; for `cursor` this includes the
  operator's full account catalog (auto, gpt-5.2, composer-2.5, claude-sonnet-5-*, …).
- Changing the Agent re-fetches and repopulates the model options and resets the selection to that
  backend's `default_model`.
- Submitting a tool session sends the selected `model`; the daemon spawns `tddy-coder --model
  <model>`. The model applies to the whole session (recipe per-goal `default_models()` hints do
  not override `context["model"]`). Selecting a backend's default reproduces today's behaviour.
- claude-cli continues to require a non-empty model (unchanged daemon precondition).

## Design decisions

### Enumerate from the command; curate only where impossible
`cursor` and the ACP backends can list their own models, so those come straight from the command.
`claude` and `codex` (non-ACP) expose no such command, so their lists are curated in `tddy-core`
(explicit and documented — not a silent fallback). One catalog mechanism (`list_models()`), two
sourcing strategies behind it.

### On-demand probe, not eager
Enumeration spawns the agent subprocess and may hit the network / require auth, so it runs lazily
per selected agent (`ListAgentModels`) rather than eagerly for every allowlisted agent inside the
cheap `ListAgents` call.

### Enumeration lives in tddy-core, invoked via tddy-tools
`CodingBackend::list_models()` keeps the ACP handshake and cursor-CLI parsing next to the backends
that own them; the daemon reuses it by shelling out to a `tddy-tools list-models` subcommand rather
than duplicating handshake logic.

### Probe failure surfaces as an error — no fallback
An enumerable backend's probe can fail (agent not logged in, binary missing). Per the repo's
no-silent-fallback rule, a failed probe is surfaced, never masked: `list_models()` returns a
`BackendError`, the daemon maps it to an RPC error, and `CreateSessionPane` renders the error
inline next to the Model select. The Model select stays empty and the Create button is disabled
for that backend until the operator resolves the underlying problem (log in / fix the binary) or
picks a different agent. No `default_model_for_agent` substitution is made — an unavailable backend
must not silently look available.

## Edge cases and constraints

- A backend that advertises a single model still renders a one-option select defaulting to it.
- Model is not independently re-validated by the daemon for tool sessions (free-form `--model`,
  consistent with `tddy-coder`'s existing `--model` contract); the UI only offers listed ids.
- No static model list remains in the in-scope session-creation surface (`CreateSessionPane`); its
  catalogs all originate from the daemon/`tddy-core`.

## Known limitations

- **Retained `CLAUDE_CLI_MODELS` for the legacy `ConnectionScreen`** — `CreateSessionPane` no longer
  reads the hardcoded `packages/tddy-web/src/constants/claudeCliModels.ts` `CLAUDE_CLI_MODELS`
  constant, but the constant is **kept** because the out-of-scope legacy `ConnectionScreen` inline
  form still populates its model dropdown from it. Removing the static list entirely is deferred to
  whenever `ConnectionScreen` is migrated to `ListAgentModels` (or retired). Its `isClaudeCliSession`
  / CLI-session helpers are unaffected.

## Related documentation

- [Session Drawer § Create Session](session-drawer.md#create-session) — the form this extends
- [Codex ACP backend](../coder/codex-acp-backend.md) — ACP `available_models` source
- [Coder overview](../coder/1-OVERVIEW.md) — backend selection and `--model`
