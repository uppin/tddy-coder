# Changeset: acp-replay-lazy-tool-bodies-web — fetch tool-call bodies on activity click

**Date:** 2026-07-25
**Branch:** `feature/lazy-activity-body/lazy-detail-web`
**Packages:** `tddy-web`
**Feature PRD:** [docs/ft/web/agent-activity-pane.md § 4 Lazy tool bodies](../../ft/web/agent-activity-pane.md#4-lazy-tool-bodies--fetch-on-click-added-2026-07-25)
**Follows:** [acp-replay-lazy-tool-bodies.md](../../ft/coder/acp-replay-lazy-tool-bodies.md) (Rust side, PR #345) — this is the web adoption named "follow-up" there.

## Problem

PR #345 stripped `raw_input`/`raw_output` out of every `StreamAcpReplay` tool frame and added a
unary `GetAcpToolCallDetail` to fetch them on demand. The web still reads the bodies off the streamed
frame (`useAcpReplay` copies `raw_input`/`raw_output` onto the tool `ChatMessage`, and
`AgentActivityDetailDialog` renders them directly), so since #345 the detail dialog shows **empty**
input/output for every tool call. The web must adopt the lookup: fetch a call's body when its row is
clicked, and cache it.

## Approach

- `useAcpReplay` stops treating the streamed frame's `raw_input`/`raw_output` as the detail source of
  truth. It carries the frame's **`tool_call_id`** onto the tool `ChatMessage` (new `toolCallId`
  field) so a click knows which call to fetch. The transcript renders from title + status alone.
- A new `useToolCallDetail({ sessionId, callId, sessionToken, client })` hook resolves one call's
  body: returns the cached `AgentActivityRegistry` body if present, otherwise fires
  `GetAcpToolCallDetail` once, writing `loading` → `loaded`/`error` into the registry. In-flight and
  loaded states are not re-fetched; an `error` state is re-fetched on a later open (retry).
- `AgentActivityRegistry` gains a per-session **body cache** keyed by `callId`
  (`getBody`/`setBody`), reference-stable for `useSyncExternalStore`.
- `AgentActivityDetailDialog` takes `sessionId`/`sessionToken`/`client`, drives `useToolCallDetail`,
  and renders a **loading** state, an **error** state, or the fetched `raw_input`/`raw_output`.
- `AgentActivityOverlay` passes the session identity + resolved client down to the dialog.
- Regenerated `connection_pb.ts` for `GetAcpToolCallDetail`.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] `tddy-web`: regenerate `src/gen/connection_pb.ts` (`bun run generate`) for `GetAcpToolCallDetail`
- [x] `tddy-web`: add `toolCallId?: string` to `ChatMessage` (`components/chat/useAgentChat.ts`)
- [x] `tddy-web`: `useAcpReplay` — set `toolCallId` on the tool `ChatMessage`; stop copying stream
  `rawInput`/`rawOutput` (`components/chat/useAcpReplay.ts`)
- [x] `tddy-web`: `AgentActivityRegistry` body cache — `ToolCallBodyState`, `getBody`/`setBody` keyed
  by `(sessionId, callId)` (`components/sessions/agentActivityRegistry.ts`)
- [x] `tddy-web`: `useToolCallDetail` hook — fetch-once + cache + retry-on-error
  (`components/sessions/useToolCallDetail.ts`)
- [x] `tddy-web`: `AgentActivityDetailDialog` — loading/error/loaded states driven by the hook
  (`components/sessions/AgentActivityDetailDialog.tsx`)
- [x] `tddy-web`: `AgentActivityOverlay` — pass `sessionId`/`sessionToken`/`client` to the dialog
  (`components/sessions/AgentActivityOverlay.tsx`)
- [x] `tddy-web`: test ids `agent-activity-detail-loading` / `agent-activity-detail-error`
  (`cypress/support/testIds.ts`); page-object helpers (`cypress/support/pages/agentActivityPage.ts`)
- [x] `tddy-web`: body-less tool-call frame builder + `GetAcpToolCallDetail` backend (deferred/error
  variants) in the replay testkit (`cypress/support/rpc/acpReplay.ts`)

## Acceptance tests

- [x] `packages/tddy-web/cypress/component/AgentActivityDetailLazyBodyAcceptance.cy.tsx` — overlay
  opens over a **body-less** stream mock; clicking a tool row triggers `GetAcpToolCallDetail`; the
  dialog shows a loading state while the fetch is in flight, then fills with the fetched
  input/output; a failed lookup shows an error state; a re-open reads the cached body without a
  second fetch.
- [x] `packages/tddy-web/cypress/component/AgentActivityDetailDialogAcceptance.cy.tsx` — rewritten so
  the existing input/output/highlight/non-tool/close cases pass over the body-less stream + detail
  RPC (no inline bodies).

## Unit tests

- [x] `packages/tddy-web/src/components/sessions/agentActivityRegistry.test.ts` — body cache:
  `getBody` undefined until set; `setBody` stores per `(sessionId, callId)`; distinct calls and
  distinct sessions don't collide; a body write notifies subscribers and leaves the transcript
  reference stable.

## Delta summary

### `tddy-web`

- `src/gen/connection_pb.ts` — regenerated: `getAcpToolCallDetail` method + `GetAcpToolCallDetail{Request,Response}` schemas.
- `src/components/chat/useAgentChat.ts` — `ChatMessage.toolCallId`.
- `src/components/chat/useAcpReplay.ts` — carry `toolCallId`, drop stream bodies from the tool entry.
- `src/components/sessions/agentActivityRegistry.ts` — `ToolCallBodyState` + `getBody`/`setBody` body cache.
- `src/components/sessions/useToolCallDetail.ts` — new fetch-once/cache/retry hook.
- `src/components/sessions/AgentActivityDetailDialog.tsx` — loading/error/loaded via the hook.
- `src/components/sessions/AgentActivityOverlay.tsx` — thread session identity + client to the dialog.
- `cypress/support/{testIds.ts,pages/agentActivityPage.ts,rpc/acpReplay.ts}` — new ids/helpers, body-less frame builder, detail backend.

## Validation Results (pr-wrap, 2026-07-25)

- **validate-changes:** 0 critical / 0 warning. One clean-up applied — removed the now-dead
  `ChatMessage.rawInput`/`rawOutput` fields (nothing sets/reads them after the switch to
  fetch-on-click; the live `useAcpSession` path uses the proto `ToolCall.fields`, not `ChatMessage`).
  Latent, practically-unreachable note: `useToolCallDetail` persists a `loading` entry to dedupe
  concurrent/StrictMode double-invokes — a client-identity churn during the in-flight unary could
  strand it, but `SessionClientCache` keeps the client stable and the fetch is a fast unary.
- **validate-tests:** fluent-tests compliant — Given/When/Then, page-object-only selectors,
  `mountWithRpc` + in-memory backend, one behavior per test, exact assertions, event-based async
  sync (no sleeps). 0 issues.
- **validate-prod-ready:** no mock/fake, no TODO/FIXME, no `console.*`/`dbg`, no error-masking
  fallbacks in the new code. 0 blockers.
- **analyze-clean-code:** score **A** — all files 75–240 lines, no must-refactor / needs-attention,
  no magic values, no production duplication.
- **Build/test gate:** `bun run build` (vite + full `tsc`, 3041 modules) clean; unit 7/7;
  Cypress Agent Activity suites 26/26 (lazy-body 6, detail-dialog 3, acp-replay 8, snapshot-in-flight
  2, lazy-count-persist 5, client-identity 2). No Rust changed — Rust gate N/A.

## Final wrap (deferred to `/merge`)

Not yet wrapped: prepend the single-line bullet to `packages/tddy-web/docs/changesets.md` and
`docs/dev/changesets.md`, add the `docs/ft/web/changelog.md` dated section (all **with the PR link**),
then delete this file. Deferred so the PR number is available at merge time.
