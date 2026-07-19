# Changeset: spawn-conversation — workflows spawn a child conversation rendered as a session tab

**Date:** 2026-07-19
**Branch:** `feat/workflow-adding-new-conversation`
**Packages:** `tddy-tools`, `tddy-core`, `tddy-daemon`, `tddy-workflow-recipes`, `tddy-web`
**Feature PRD:** [docs/ft/coder/spawn-conversation.md](../../ft/coder/spawn-conversation.md)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] `tddy-core`: add `SpawnConversationRequestWire` (`toolcall/mod.rs`); re-export it
- [x] `tddy-core`: add `ConversationSpawnHandler` trait + `with_conversation_spawn_handler` builder + `conversation_spawn_handler` field (`toolcall/listener.rs`)
- [x] `tddy-core`: add `"SpawnConversation"` to the dispatch allowlist + `handle_spawn_conversation` (`toolcall/listener.rs`)
- [x] `tddy-tools`: add `SpawnConversationInput` + `spawn_conversation` tool + pure `spawn_conversation_request_json` builder fn (`server.rs`)
- [x] `tddy-daemon`: add `GrillMeConversationSpawnHandler` + `conversation_branch_slug` (`connection_service.rs`)
- [x] `tddy-daemon`: thread `conversation_spawn_handler` through `spawn_claude_cli_session_inner` → `prepare_managed_workflow_inner` → `set_up_managed_workflow`/`resume_managed_workflow`/`build_managed_workflow` → `start_session_toolcall_listener` (`connection_service.rs`, `session_toolcall.rs`)
- [x] `tddy-daemon`: `recipe_enables_conversation_spawn` + `conversation_spawn_handler_for` selection; bind conversation handler for `grill-me` in `start_claude_cli_session` + `start_sandboxed_claude_cli_session`
- [x] `tddy-workflow-recipes`: add the `spawn_conversation` "Hand off to implementation" step to `create_plan_system_prompt` (`grill_me/prompt.rs`); commit-`plans/<slug>.md`-first instruction
- [x] `tddy-workflow-recipes`: `goal_hints("create-plan").allowed_tools` left `vec![]` (empty = allow-all; no gating change needed)
- [x] `tddy-web`: `useChildSessions` deriving children from the session list (`components/sessions/useChildSessions.ts`)
- [x] `tddy-web`: render child tabs in `SessionTerminalTabs.tsx`; add `sessions-child-tab-<id>` testid
- [x] `tddy-web`: mount a selected child's runtime pane in `SessionRuntime.tsx`; feed `childSessions` from `SessionMainPane`/`SessionsDrawerScreen`

## Acceptance tests

- [x] `packages/tddy-web/cypress/component/SessionChildTabsAcceptance.cy.tsx` — 3/3 passing (existing `SessionTerminalTabsAcceptance` 5/5, no regression)

## Unit tests

- [x] `packages/tddy-core/src/toolcall/mod.rs` — `SpawnConversationRequestWire` deserialization (2 tests)
- [x] `packages/tddy-core/src/toolcall/listener.rs` — dispatch through a bound handler; reject when unbound (2 tests)
- [x] `packages/tddy-tools/src/server.rs` — errors without `TDDY_SOCKET`; relayed request shape (2 tests)
- [x] `packages/tddy-daemon/src/connection_service.rs` — `recipe_enables_conversation_spawn` selection (grill-me vs tdd/pr-stack)
- [x] `packages/tddy-workflow-recipes/src/grill_me/prompt.rs` — Create-plan prompt names `spawn_conversation`

_Note: the daemon `orchestrator_session_id` contract is reused verbatim from the existing `spawn-child` path (already covered by `session_list_enrichment` tests), so it is not re-pinned here; the `spawn_conversation` handler simply calls the same `spawn_claude_cli_session_inner(stack_parent=...)`._

## Delta summary

### `tddy-core`

**Modified files:**
- `src/toolcall/mod.rs` — new `SpawnConversationRequestWire { r#type, prompt, branch: Option<String>,
  base_ref: Option<String> }` (parallel to `SpawnChildRequestWire`); re-export it alongside
  `ConversationSpawnHandler`. Reuses `ToolCallResponse::SpawnChildOk { session_id }` — no new response
  variant.
- `src/toolcall/listener.rs` — new `trait ConversationSpawnHandler { async fn spawn_conversation(&self,
  prompt: &str, branch: Option<&str>, base_ref: Option<&str>) -> Result<String, String>; }`; new
  `conversation_spawn_handler` field on `ToolcallRpcService` + `with_conversation_spawn_handler`
  builder; `"SpawnConversation"` added to the dispatch allowlist + arm; `handle_spawn_conversation`
  mirroring `handle_spawn_child` (reject with `Error{message}` when no handler bound).

### `tddy-tools`

**Modified files:**
- `src/server.rs` — new `SpawnConversationInput { prompt, branch: Option<String>, base_ref:
  Option<String> }`; new `spawn_conversation` `#[tool]` method (not pr-stack-gated) that guards on
  `permission_relay_socket_path()`, builds the request via a small pure fn
  (`spawn_conversation_request_json`) so it is unit-testable, and relays via
  `toolcall_client::dispatch_toolcall`.

### `tddy-daemon`

**Modified files:**
- `src/connection_service.rs` — new `GrillMeConversationSpawnHandler` (`impl ConversationSpawnHandler`)
  calling `spawn_claude_cli_session_inner(new_branch_from_base, stack_parent = Some(parent),
  managed_recipe = None)`. New `conversation_spawn_handler: Option<Arc<dyn ConversationSpawnHandler>>`
  parameter on `spawn_claude_cli_session_inner` + `prepare_managed_workflow_inner`. New helper
  `recipe_spawn_handlers(recipe, …) -> (Option<Arc<dyn ChildSpawnHandler>>, Option<Arc<dyn
  ConversationSpawnHandler>>)`; bound for `grill-me` in `start_claude_cli_session` +
  `start_sandboxed_claude_cli_session` (mirrors the existing `pr-stack` child-handler block).
- `src/session_toolcall.rs` — `conversation_spawn_handler` parameter added to `set_up_managed_workflow`,
  `resume_managed_workflow`, `build_managed_workflow`, and `start_session_toolcall_listener`; bound via
  `.with_conversation_spawn_handler(...)` alongside `.with_child_spawn_handler(...)`.

### `tddy-workflow-recipes`

**Modified files:**
- `src/grill_me/prompt.rs` — `create_plan_system_prompt` gains a required final step: after both brief
  files exist, commit `plans/<slug>.md`, then call `tddy-tools spawn_conversation` from a shell with a
  prompt referencing the absolute session-artifact brief path + `plans/<slug>.md` and a `branch`.
- `src/grill_me/mod.rs` — if `goal_hints("create-plan").allowed_tools` gates MCP availability, add the
  spawn tool name (currently `vec![]`). `goal_requires_tddy_tools_submit` stays `false`.

### `tddy-web`

**New files:**
- `src/components/sessions/useChildSessions.ts` — derives `childSessions = sessions.filter(s =>
  s.orchestratorSessionId === parentSessionId)` from the drawer session list.

**Modified files:**
- `src/components/sessions/SessionTerminalTabs.tsx` — accept `childSessions` + `onSelectChild`; render a
  tab per child (testid `sessions-child-tab-<sessionId>`) after bash tabs, before `+`.
- `src/components/sessions/SessionRuntime.tsx` — accept `childSessions`; extend the active-tab state to a
  discriminated union (`agent`/`bash`/`child`); attach + mount the selected child session's runtime pane.
- `src/components/sessions/SessionMainPane.tsx` / `SessionsDrawerScreen.tsx` — pass the enriched session
  list down so `SessionRuntime` can derive its children.

**Test support:**
- `cypress/support/pages/sessionTerminalTabsPage.ts` — `childTab(id)`, `childPane(id)`.
- `cypress/support/testIds.ts` — `sessionsChildTab(id)` → `sessions-child-tab-<id>`.
- `cypress/support/rpc/connectionServiceBackend.ts` — ensure `aSessionEntry` accepts an
  `orchestratorSessionId` override (proto field already exists — no new RPC).

## Validation Results

- **PR-wrap (validate-changes / tests / prod-ready / clean-code):** 0 critical, 0 warning. Info only: recursive-child-runtime (safe, DAG), an orphaned-pane cosmetic edge case, and a stray unrelated `resume.sh` to exclude from the commit. No new TODO/FIXME, no `println!`/`unwrap` in production, tests fluent-compliant.
- **Lint:** `cargo fmt --check` clean; `cargo clippy -p tddy-core -p tddy-tools -p tddy-workflow-recipes -- -D warnings` clean. (`tddy-daemon` clippy skipped — webrtc rebuild would re-fill disk; crate compiles clean via its test run.)
- **Web (Cypress component):** `SessionChildTabsAcceptance.cy.tsx` 3/3 passing; `SessionTerminalTabsAcceptance.cy.tsx` 5/5 (no regression).
- **Rust (per-crate):** `tddy-core` + `tddy-tools` + `tddy-workflow-recipes` — all green (exit 0), including every `pr_stack_*` / `orchestrate_pr_stack_*` acceptance test (PR-stack path untouched). `tddy-daemon` — `conversation_spawn_wiring_tests` passes and the crate compiles with the threaded handler.
- **Note:** the full workspace `./test` (which additionally rebuilds the webrtc/aws-lc-heavy `tddy-livekit`/`tddy-coder` stacks) could not complete in this environment due to `/var` disk exhaustion (`os error 28`), not any code failure; verification was done per affected crate instead. `tddy-coder` only consumes `tddy-core`'s additive exports and is unaffected by the daemon-internal signature threading.

## Non-goals

- No auto-run managed recipe in the child (it is a plain interactive claude-cli conversation).
- No tab rename/reorder or cross-restart tab persistence (inherits the terminal-tabs PRD non-goals).
- No change to the PR-stack `spawn-child` tool, wire type, handler, or verb.
