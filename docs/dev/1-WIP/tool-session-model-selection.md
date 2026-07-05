# Changeset: Tool-session model selection

**PRD**: `docs/ft/web/tool-session-model-selection.md`
**Branch**: `model-selection`

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit/integration tests
- [x] Add `ListAgentModels` RPC to proto + regenerate (Rust + TS)
- [x] tddy-core: `list_models()` trait method + `BackendModels` types
- [x] tddy-core: cursor `--list-models` enumeration + parser
- [x] tddy-core: ACP `available_models` enumeration (claude-acp, codex-acp)
- [x] tddy-core: curated lists (claude, codex, claude-cli, stub)
- [x] tddy-tools: `list-models --agent` subcommand
- [x] tddy-daemon: `ListAgentModels` handler (+ cache) shelling out to tddy-tools
- [x] tddy-daemon: thread `StartSessionRequest.model` → `--model` for tool sessions
- [x] tddy-web: on-demand model select in `CreateSessionPane` (tool + claude-cli)
- [~] tddy-web: remove hardcoded `CLAUDE_CLI_MODELS` — removed from `CreateSessionPane`; retained for
  the out-of-scope legacy `ConnectionScreen` (see note under Design decisions)

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-tools/src/list_models.rs` | `run_list_models(agent)` — build backend, call `list_models()`, print JSON |
| `packages/tddy-web/src/rpc/useAgentModels.ts` | Hook: fetch `ListAgentModels(agent)` with loading/error state |
| `packages/tddy-web/cypress/component/CreateSessionModelSelectionAcceptance.cy.tsx` | Acceptance tests (in-memory RPC backend) |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-service/proto/connection.proto` | Add `rpc ListAgentModels`, `ListAgentModelsRequest/Response`, `message ModelInfo` |
| `packages/tddy-core/src/backend/mod.rs` | `BackendModel`/`BackendModels` types; `CodingBackend::list_models()` (async, default = curated); `AnyBackend`/`SharedBackend` dispatch; `curated_models_for_agent`; `claude_cli_models()` |
| `packages/tddy-core/src/backend/cursor.rs` | `list_models()` — spawn `<bin> --list-models`, parse `id - label` + `(current, default)` |
| `packages/tddy-core/src/backend/acp.rs` | `list_models()` — ACP `initialize`+`new_session`, read `available_models` (stop discarding `.models`) |
| `packages/tddy-core/src/backend/codex_acp.rs` | `list_models()` — same ACP `available_models` path |
| `packages/tddy-core/src/backend/claude.rs` | `list_models()` — curated list |
| `packages/tddy-core/src/backend/codex.rs` | `list_models()` — curated list |
| `packages/tddy-core/src/backend/stub.rs`, `mock.rs` | `list_models()` — static |
| `packages/tddy-tools/src/main.rs` | Register `ListModels` subcommand |
| `packages/tddy-tools/src/cli.rs` | `ListModelsArgs` (agent id + optional cli-path overrides) |
| `packages/tddy-daemon/src/connection_service.rs` | `list_agent_models` handler (+ per-(agent,daemon) TTL cache); thread `req.model` into tool `SpawnOptions` |
| `packages/tddy-daemon/src/spawner.rs` | `SpawnOptions.model`; append `--model <m>` when non-empty |
| `packages/tddy-daemon/src/spawn_worker.rs` | `SpawnOptions`/`SpawnRequest` `model` field; `build_spawn_request` passthrough |
| `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx` | Model select for tool sessions; fetch via `useAgentModels`; send `model` for tool; feed claude-cli select from RPC; error/loading states |
| `packages/tddy-web/src/constants/claudeCliModels.ts` | Remove `CLAUDE_CLI_MODELS`; keep `isClaudeCliSession` |
| `packages/tddy-web/src/gen/connection_pb.ts` | Regenerated (new RPC + messages) |
| `packages/tddy-web/cypress/support/rpc/responses.ts` | `listAgentModels(...)` proto response builder |
| `packages/tddy-web/cypress/support/rpc/connectionServiceBackend.ts` | Seed `listAgentModels` in the in-memory backend |
| `packages/tddy-web/cypress/support/testIds.ts` | Add ids for model loading / error states if needed |

## Design decisions

### Enumerate from the command; curate only where impossible
`cursor` (`--list-models`) and the ACP backends (`available_models`) enumerate their own models
from the underlying command. `claude` and `codex` (non-ACP) expose no such command, so their lists
are curated in `tddy-core` — one `list_models()` mechanism, two sourcing strategies behind it.

### On-demand probe via a dedicated RPC
`ListAgentModels(agent)` runs the subprocess probe lazily for the selected backend only; the cheap
`ListAgents` call is unchanged. Results cached per (agent, daemon) with a short TTL.

### Enumeration in tddy-core, invoked via tddy-tools
`CodingBackend::list_models()` keeps ACP-handshake + cursor parsing next to the backends. The
daemon reuses it by shelling out to `tddy-tools list-models --agent X` rather than duplicating.

### Probe failure surfaces as an error — no fallback
A failed probe (not logged in / binary missing) returns a `BackendError` → RPC error → inline error
in the form; the Model select stays empty and Create is disabled for that backend. No
`default_model_for_agent` substitution.

### Single session-wide model
The chosen `model` becomes `tddy-coder --model`, seeding `context["model"]` for every invoke.
Recipe per-goal `default_models()` hints do not override it.

## Acceptance tests

Cypress component tests, in-memory RPC backend (`anInMemoryRpcBackend` from `tddy-connectrpc-testkit`),
`packages/tddy-web/cypress/component/CreateSessionModelSelectionAcceptance.cy.tsx`:

1. **shows a model dropdown for tool sessions populated from the selected agent's advertised models** —
   backend seeds `listAgentModels("cursor")` → the select lists those ids/labels.
2. **preselects the agent's default model** — the select's value equals the response `default_model`.
3. **repopulates and resets the model when the agent changes** — switching agent re-calls
   `ListAgentModels` and the select shows the new agent's models, value = new default.
4. **sends the selected model in StartSession for a tool session** — `callsTo(startSession)[0].model`
   equals the picked id; `sessionType === ""`.
5. **populates the claude-cli model dropdown from the daemon (no hardcoded list)** — switching to
   Claude CLI calls `ListAgentModels("claude-cli")` and lists the returned models.
6. **shows an error and disables Create when the model probe fails** — `listAgentModels` errors →
   inline error visible, Model select empty, Create button disabled.

## Unit/integration tests

**tddy-core — cursor parser** (`packages/tddy-core/src/backend/cursor.rs`):
1. parses `--list-models` output into `(id, label)` pairs, skipping the header/blank lines
2. treats the `(current, default)` line as the default model and strips the marker from its label

**tddy-core — curated catalogs** (`packages/tddy-core/src/backend/mod.rs`):
3. `curated_models_for_agent("claude")` lists opus/sonnet/haiku with `opus` default
4. `curated_models_for_agent("codex")` lists gpt-5 with `gpt-5` default
5. `claude_cli_models()` lists the full claude ids with `claude-opus-4-8` default

**tddy-core — ACP enumeration** (`acp_models_from_session_state`, `backend/mod.rs`):
6. maps `SessionModelState.available_models` with `current_model_id` as the default
7. errors when the agent advertised no `SessionModelState` (no fallback to an empty list)

**tddy-tools — JSON contract** (`packages/tddy-tools/src/list_models.rs`):
8. `render_models_json` renders a `BackendModels` catalog as the daemon⇄tools JSON contract

**tddy-daemon — spawn threading** (`packages/tddy-daemon/src/spawn_worker.rs`):
9. `build_spawn_request` carries `model` into the spawn-worker request (→ `--model` for the child)

**tddy-daemon — JSON contract** (`parse_agent_models_json`, `connection_service.rs`):
10. reads the models + default from the `tddy-tools` JSON
11. errors on malformed probe output (a failed probe is never an empty catalog)

> Note: ACP enumeration is red-tested via the pure `acp_models_from_session_state` mapping (unit,
> no subprocess) rather than a `tddy-acp-stub` integration test; the daemon⇄tools boundary is
> tested via the paired `render_models_json` / `parse_agent_models_json` contract functions rather
> than a fake-binary shell-out. The `--model` argv emission and the full `ListAgentModels` RPC
> wiring are green-phase work exercised by the acceptance tests.

## Out of scope

- `ConnectionScreen` inline session form (legacy; not retired) — unchanged.
- Changing which model a **running** session uses (no `SetSessionModel` surface here).
- Per-goal model overrides via the UI.

## Validation Results

- **clippy** (`-D warnings`, tddy-core / tddy-tools / tddy-daemon, `--all-targets`): clean.
- **tddy-web** typecheck + `bun run build`: clean.
- **Tests**: tddy-core 236/236, tddy-tools 29/29, tddy-daemon 250 pass with 1 pre-existing
  failure — `sandbox_session::tests::dial_and_bridge_…` guards on the `tddy-sandbox-runner` binary
  existing (built by the repo's `./test`, not by `cargo test -p tddy-daemon`); the file is untouched
  by this change and the failure reproduces identically without it. Cypress
  `CreateSessionModelSelectionAcceptance` 7/7, `CreateSessionPane` 29/29; other modified
  `CreateSession*` specs green per-spec.
- **Diff review** (production-readiness + correctness + fluent-tests): no mock/hardcoded logic, no
  leftover markers, probe failures surfaced (no fallback), web hook guards stale responses. Two
  correctness fixes applied:
  - Model cache now keyed by **OS user** (cursor/ACP catalogs are account-specific — prevents
    cross-user cache leakage on a multi-tenant daemon).
  - `run_capture_as_user` sets the probe's `current_dir` to the user's **home** (the daemon cwd may
    be unreadable after setuid; the ACP probe opens a session against the cwd).
- Reverted unrelated churn (`tddy-sandbox-recipes/src/lib.rs` fmt drift, `buildId.ts` timestamp).
