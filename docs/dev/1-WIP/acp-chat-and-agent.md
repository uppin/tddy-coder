# Changeset: Reusable Agent Chat + tddy-coder as an ACP agent

**PRD**: `docs/ft/coder/acp-agent.md`, `docs/ft/web/session-drawer.md` (§ Agent Chat)
**Branch**: `feat/acp-chat-1`

Two goals in one changeset:

1. **Extract the PR-Stack chat** into a recipe-agnostic `AgentChat` / `useAgentChat` (frontend,
   `TddyRemote` wire protocol unchanged).
2. **Expose the workflow as an ACP agent** (`tddy-coder --acp`) and make the session-host drive it
   over ACP, bridging back to the web's existing `TddyRemote` stream. Coding backends stay additive
   (all six kept; ACP is the default). Architecture: `web --TddyRemote/LiveKit--> host --ACP--> tddy-coder --acp`.

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit/integration tests
- [x] Frontend: extract `AgentChat` + `useAgentChat`; repoint `PrStackScreen`; `agent-chat-*` ids
- [x] `tddy-acp` crate: `mapping` module (forward + inverse). Unified `AcpClient` dedup deferred (see notes)
- [x] `tddy-coder --acp`: `acp::Agent` over `WorkflowEngine`
- [x] Session-host bridge (`acp_host.rs`): ACP `session/update` → `PresenterEvent` (additive/opt-in)
- [~] Daemon spawn: host-default rewire deferred — `TODO(acp-host-rewire)`, needs LiveKit host validation
- [ ] Extend `backend_is_agent_driven` so ACP is the default path — deferred with the host rewire
- [ ] Storybook `AgentChat.stories.tsx` — deferred (not test-backed)

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-web/src/components/chat/AgentChat.tsx` | Reusable chat component (from `PrStackChat`), `placeholder`/`title` props, `agent-chat-*` ids |
| `packages/tddy-web/src/components/chat/useAgentChat.ts` | Reusable hook (from `usePresenterChat`), unchanged behavior |
| `packages/tddy-web/src/components/chat/AgentChat.stories.tsx` | Storybook: empty / streaming / select / multiSelect / error / connecting |
| `packages/tddy-web/cypress/support/pages/agentChatPage.ts` | Page object over `agent-chat-*` ids |
| `packages/tddy-web/cypress/component/AgentChatStreamingAcceptance.cy.tsx` | Acceptance — standalone streaming merge |
| `packages/tddy-web/cypress/component/AgentChatElicitationAcceptance.cy.tsx` | Acceptance — select / multi-select / other |
| `packages/tddy-web/cypress/component/AgentChatReusableConfigAcceptance.cy.tsx` | Acceptance — placeholder/title, no `SessionEntry`, `agent-chat-*` ids |
| `packages/tddy-acp/` (crate) | Shared ACP: `AcpClient` (dedup), `agent` (workflow agent), `mapping` (event/intent ↔ ACP) |
| `packages/tddy-coder/src/acp_agent.rs` | `--acp` mode: `AgentSideConnection` over stdio + worker-thread bridge to `WorkflowEngine` |
| `packages/tddy-coder/src/acp_host.rs` | Session-host bridge: ACP client ⇄ `TddyRemote`/Presenter surface |
| `packages/tddy-integration-tests/tests/tddy_coder_acp_agent_acceptance.rs` | Acceptance — drive `tddy-coder --acp` via an ACP client |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-web/src/components/sessions/prstack/PrStackScreen.tsx` | Render `<AgentChat placeholder=… />` instead of `PrStackChat` |
| `packages/tddy-web/src/components/sessions/prstack/PrStackChat.tsx`, `usePresenterChat.ts` | Removed (moved to `components/chat/`) |
| `packages/tddy-web/cypress/support/testIds.ts` | Add `agentChat*` ids + `agentChatMessage/Option/MultiSelectOption` helpers |
| `packages/tddy-web/cypress/component/PrStackChat*.cy.tsx`, `cypress/support/pages/prStackScreenPage.ts` | Repoint chat selectors to `agent-chat-*` (behavior unchanged, stay green) |
| `Cargo.toml` (root) | Add `packages/tddy-acp` to workspace members |
| `packages/tddy-core/src/backend/acp.rs`, `codex_acp.rs` | Reduce to thin config over `tddy-acp::AcpClient` |
| `packages/tddy-coder/src/run.rs` | `--acp` arg + dispatch (before TUI/plain fork); daemon path drives the host bridge |
| `packages/tddy-coder/src/lib.rs` | Register `acp_agent`, `acp_host` modules |
| `packages/tddy-core/src/presenter/agent_session_runner.rs` | Extend `backend_is_agent_driven` so ACP is the default single-session path |
| `packages/tddy-daemon/src/spawner.rs`, `spawn_worker.rs`, `action_service.rs` | Spawn the ACP-host session process that spawns `tddy-coder --acp` |

## Design decisions

### Web wire protocol is kept; ACP inserted at the agent boundary
The browser keeps `TddyRemote.Stream` over LiveKit — the entire web transport + Presenter
view-adapter is preserved. ACP sits only between the session-host and the workflow agent. This is
why the reusable `AgentChat` is a `TddyRemote` client, not literally an ACP client.

### One unified ACP client
`backend/acp.rs` and `backend/codex_acp.rs` are ~95% duplicated. They collapse onto a single
`tddy-acp::AcpClient` parameterized by agent-spawn config; `ClaudeAcpBackend` / `CodexAcpBackend`
become thin wrappers. The same crate houses the agent side (`--acp`) and the pure `mapping` module.

### Additive backends — no removal
`--agent` still accepts `claude` / `cursor` / `codex` / `claude-acp` / `codex-acp` / `stub`. The
change makes ACP the **default** single-session path (`backend_is_agent_driven`), not the only one.
No `stream/{claude,cursor,codex}.rs` files are removed.

### Bridge reuses existing surfaces
The host bridge reuses `TddyRemoteService` + the Presenter view-adapter (web side) and the ACP
client machinery (agent side). Only the *producer* of the web events changes from an in-process
`WorkflowEngine` to the ACP agent child.

## Acceptance tests

**Frontend** — Cypress component, in-memory RPC backend (`aSessionsDrawerBackend` + `mountWithRpc`,
`RpcTransportProvider liveKitFactory` override, `room={null}`):

1. `AgentChatStreamingAcceptance` — **merges streamed agent tokens into a single bubble** — standalone
   `AgentChat` (not via `PrStackScreen`); token deltas + a duplicate full-line snapshot render as one
   bubble.
2. `AgentChatElicitationAcceptance` — **renders a select question and answers it** / **submits checked
   multi-select options** / **submits a custom Other answer** — a `ModeChanged` select/multiSelect
   drives the question panel; answering enqueues the matching intent.
3. `AgentChatReusableConfigAcceptance` — **uses the provided placeholder** and **exposes `agent-chat-*`
   test ids with no `SessionEntry` dependency** — proves recipe-agnostic reuse.

**Backend** — `tddy_coder_acp_agent_acceptance.rs`, an ACP `ClientSideConnection` drives the real
`tddy-coder --acp` binary (a `stub` coding backend + scripted feature so no external agent is spawned):

4. **advertises workflow-agent capabilities and models on initialize**
5. **streams agent output as AgentMessageChunk notifications during a prompt**
6. **raises a permission request for a clarification question and advances when answered**
7. **returns EndTurn when the workflow completes**
8. **resumes an existing session via load_session**

## Unit/integration tests

`tddy-acp` crate (`mapping` module — pure functions, no subprocess):

1. `PresenterEvent::AgentOutput` → `SessionUpdate::AgentMessageChunk`
2. `PresenterEvent::ActivityLogged` (ToolUse) / `ProgressEvent::ToolUse` → `SessionUpdate::ToolCall`
3. `ClarificationQuestion` (select) → `RequestPermissionRequest` with one option per choice
4. `ClarificationQuestion` (multi-select, allow_other) → permission options incl. an Other affordance
5. `ExecutionStatus::Completed` → `StopReason::EndTurn`; `Error` → prompt error
6. **inverse (bridge):** a `SessionUpdate::AgentMessageChunk` → `ServerMessage::AgentOutput`; a
   permission `select` answer → the matching ACP option id
7. `AcpClient` parity: `claude-acp` and `codex-acp` spawn configs drive identical client behavior

> Note: the host-bridge and daemon-spawn end-to-end paths (real LiveKit + fork/exec) are verified
> manually via `./web-dev` (see PRD verification) rather than a flaky integration harness at red; the
> bridge's translation logic is red-tested via the pure `mapping` inverse functions (test 6).

## Out of scope

- Removing / deprecating the non-ACP coding backends.
- Changing the browser wire protocol to ACP (kept as `TddyRemote`).
- A single-process host multiplexing multiple agents.

## Validation Results

**Tests (all green):**
- `tddy-acp` mapping — 8/8 (`cargo test -p tddy-acp --lib`).
- `tddy_coder_acp_agent_acceptance` — 3/3 (drives the real `tddy-coder --acp` binary).
- `acp_host_bridge_acceptance` — 4/4 (bridge translation via `tddy-acp-stub` + spawn-args contract).
- Frontend Cypress — new `AgentChat*` 8/8; existing PrStack chat 32/32; other PrStack screens 21/21.
- `tddy-coder` full suite — 0 failures (no regressions).
- clippy `-D warnings` clean (tddy-acp, tddy-coder); `cargo fmt` applied.

**Scope delivered vs. deferred:**
- Delivered: reusable `AgentChat`/`useAgentChat`; `tddy-acp` mapping crate; `tddy-coder --acp` agent;
  additive `AcpHostBridge` (ACP `session/update` → `PresenterEvent`).
- Deferred (need a real LiveKit/daemon host to validate — not verifiable in-sandbox):
  flipping the per-session host to run fully via the bridge by default (`TODO(acp-host-rewire)`),
  the daemon-spawn change, extending `backend_is_agent_driven`, and the `AcpClient` dedup of
  `acp.rs`/`codex_acp.rs`. `PrStackChat.tsx` remains a thin adapter over `AgentChat` (4 existing
  specs mount it directly); `AgentChat.stories.tsx` not written (not test-backed).

**Notable test edits during green:** 3 mechanical compile fixes to the red-phase
`tddy_coder_acp_agent_acceptance.rs` (missing `Agent` trait import, `PermissionOption.option_id`
field name, a `cwd` borrow-after-move) — no assertion or behavior changed.

**PR-wrap review + fixes (Rust + frontend review agents):**
- Fixed (major, Rust): `--acp` now runs `enforce_stdio_safe_log_output` alongside `--stdio` — a
  config `output: stdout` logger would otherwise corrupt the ACP JSON-RPC stream (`run.rs`).
- Fixed (minor, Rust): `AcpHostClient::request_permission` selects the agent's first *offered*
  option id (was a hardcoded `"allow-once"` that may match nothing); denies via `Cancelled` when
  none offered (`acp_host.rs`).
- Fixed (major, frontend): removed duplicated placeholder logic — `PrStackScreen` renders
  `<PrStackChat session=…>` (the adapter) instead of `<AgentChat placeholder=…>`, so the pr-stack
  placeholder derivation lives in one place and the adapter has a production caller.
- Fixed (minor, frontend): dropped the unused speculative `title` prop from `AgentChat`; generalized
  its `roomStatus` doc; removed the orphaned `prStackChat*` test ids/helpers from `testIds.ts`.
- Removed a stray machine-specific `packages/tddy-coder/.cursor/mcp.json` accidentally created
  during the work (not part of the feature).
- Re-verified after fixes: clippy `-D warnings` clean; `tddy-acp` 8/8, agent 3/3, bridge 4/4;
  affected Cypress specs 14/14 (AgentChat*, PrStackViewRouting, PrStackChatAcceptance); `cargo fmt`
  clean. Full-workspace `./test` not run (localized change; per-package suites green).

Remaining review items intentionally left as scoped TODOs (need the deferred host rewire /
LiveKit-host validation): `acp_agent.rs` `cancel` interrupting an in-flight turn + the detached
workflow worker lifetime; the shared `LocalSet` worker-thread helper to dedup `acp_agent.rs` /
`acp_host.rs` / `backend/acp.rs`.
