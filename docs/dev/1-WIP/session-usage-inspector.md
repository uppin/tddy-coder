# Changeset: session-usage-inspector — real-time per-session token usage in the web Inspector

**Date:** 2026-07-12
**Branch:** `feat/real-time-usage-ui`
**Packages:** `tddy-service`, `tddy-core`, `tddy-daemon`, `tddy-web`
**Feature PRD:** [docs/ft/web/session-usage-inspector.md](../../ft/web/session-usage-inspector.md)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Add `TokenUsageUpdated` + `ConversationRecord` messages and `token_usage_updated = 13` to the `ServerMessage` oneof (`tddy-service/proto/tddy/v1/remote.proto`); regenerate Rust + TS
- [x] Add `PresenterEvent::TokenUsageUpdated(Vec<ConversationRecord>)` (`tddy-core/src/presenter/presenter_events.rs`)
- [x] Map the new event in `event_to_server_message` (`tddy-service/src/convert.rs`)
- [x] Extract `gather_session_usage(...)` from `print_token_summary` into reusable form (`tddy-core/src/backend/mod.rs`); reuse the existing `read_claude_transcript_usage` / `read_claude_subagent_usages` + `accounting.json` merge
- [x] `SessionUsageEmitter` (broadcast + dedup) + `spawn_usage_watcher` poll-based watcher (`tddy-daemon/src/usage_watcher.rs`)
- [ ] **Blocked:** wire `spawn_usage_watcher` into the interactive/CLI session spawn path + replay the current usage snapshot on connect (`tddy-service/src/service.rs`). Depends on the unfinished per-session presenter stream (LiveKit `Room` not yet threaded — see `usePresenterChat.ts:98` TODO). `spawn_usage_watcher` is implemented ready-to-wire; wire-in point marked with `TODO` in `usage_watcher.rs`.
- [x] Web: `formatTokens` (`tddy-web/src/components/sessions/formatTokens.ts`)
- [x] Web: `sessionUsage.ts` pure helpers — `emptyUsage()`, `usageTotals(records)`
- [x] Web: `useSessionUsage(room, serverIdentity)` hook (`tddy-web/src/components/sessions/useSessionUsage.ts`)
- [x] Web: `SessionUsageTab` component (`tddy-web/src/components/sessions/SessionUsageTab.tsx`)
- [x] Web: add `"usage"` to `InspectorTab` + Usage button (`InspectorTabs.tsx`), render tab (`SessionInspectorDrawer.tsx`), thread `serverIdentity` from `SessionMainPane.tsx`

## Acceptance tests

- [x] `packages/tddy-web/cypress/component/SessionInspectorUsageAcceptance.cy.tsx` — 5/5 passing

## Unit / integration tests

- [x] `packages/tddy-service/src/convert.rs` — `event_to_server_message(TokenUsageUpdated)` mapping (1/1)
- [x] `packages/tddy-core/src/backend/mod.rs` — `gather_session_usage` merge + missing-transcript zero row (2/2)
- [x] `packages/tddy-daemon/tests/usage_watcher.rs` — `SessionUsageEmitter` snapshot-on-first-gather / re-emit-on-change / dedup (3/3)
- [x] `packages/tddy-web/src/components/sessions/formatTokens.test.ts`
- [x] `packages/tddy-web/src/components/sessions/sessionUsage.test.ts`

## Validation Results

### PR-readiness review (2026-07-12)

**Blockers:** none. Rust refactor preserves prior behavior; emitter dedup contract correct; wire mapping field-for-field; tests fluent-compliant with exact-value assertions.

**Should-fix (addressed):**
- `useSessionUsage` opened the `TddyRemote.Stream` on inspector-drawer mount, which would double-subscribe alongside `usePresenterChat` once the server side is wired. **Fixed:** the hook now lives inside `SessionUsageTab`, so the stream opens only while the Usage tab is mounted.

**Hardening (addressed):**
- Added `omits_the_main_agent_and_its_subagents_when_include_main_agent_is_false` to `gather_session_usage` tests (the accounting-only / Cursor-session branch).

**Accepted / deferred (not blockers):**
- `spawn_usage_watcher` / `UsageWatchTarget` are implemented but unwired (dead in production) pending the per-session presenter-stream LiveKit `Room` threading — documented `TODO` in `usage_watcher.rs`. Until wired, the Usage tab is fully built + tested but shows no live data in production.
- Minor style nits (a couple of multi-`expect` boundary tests) left as-is — boundary cases of one behavior.

## Delta summary

### `tddy-service`

- **proto** `tddy/v1/remote.proto` — new `TokenUsageUpdated` + `ConversationRecord` messages; add
  `token_usage_updated = 13` to `ServerMessage.oneof event`.
- **`src/convert.rs`** — new arm in `event_to_server_message` mapping
  `PresenterEvent::TokenUsageUpdated` → proto, field-for-field from `tddy_core::ConversationRecord`.
- **`src/service.rs`** — replay current usage snapshot to a newly-connected stream (the stream
  otherwise forwards future broadcasts only).

### `tddy-core`

- **`src/presenter/presenter_events.rs`** — `PresenterEvent::TokenUsageUpdated(Vec<ConversationRecord>)`.
- **`src/token_accounting.rs`** — `gather_session_usage(...)`: merge main-agent transcript, Claude
  Task-subagent transcripts, and parsed `accounting.json` into one ordered `Vec<ConversationRecord>`
  (extracted from `tddy-sandbox-app`'s `print_token_summary`, which then calls it).

### `tddy-daemon`

- **`src/`** — per-session usage file-watcher spawned alongside the session; watches transcript +
  subagent transcripts + `accounting.json`, calls `gather_session_usage`, broadcasts
  `PresenterEvent::TokenUsageUpdated` on start and on debounced change.

### `tddy-web`

- **New:** `formatTokens.ts`, `sessionUsage.ts`, `useSessionUsage.ts`, `SessionUsageTab.tsx`.
- **Modified:** `InspectorTabs.tsx` (`"usage"` tab), `SessionInspectorDrawer.tsx` (render tab).
- **Test infra:** `cypress/support/rpc/usageBackend.ts` (`aSessionUsageBackend`),
  `src/test-utils/builders.ts` (`aConversationRecord`), `cypress/support/pages/sessionsDrawerPage.ts`
  + `cypress/support/testIds.ts` (usage tab/row/total/empty helpers).
</content>
