# Changeset: web-url-state-routing — every navigable selection round-trips through the URL

**Date:** 2026-08-01
**Branch:** `fix-update-web-url-state`
**Packages:** `tddy-web`
**Feature PRD:** [docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md](../ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md)

## Technical delta

### New: a location model, not a path string

`usePathname()` in `src/index.tsx` returns `[string, (to: string) => void]` — a bare hash path with
no query support and no way for a nested screen to reach it (every screen receives `onNavigate` by
prop, and `SessionsDrawerScreen` writes `window.location.hash` directly in one place, bypassing it).
That shape cannot carry `?host=…&inspector=…`, so it is replaced by two new modules:

- **`src/routing/appLocation.ts`** — pure, no DOM. `AppLocation = { path, params }`;
  `parseAppLocation(hash)`, `formatAppLocation(loc)`, `withParams(loc, patch)` (a `null` value
  deletes), `withPath(loc, path)` — which distinguishes a **screen change** (a different first path
  segment: drops every screen-scoped param, keeps `host`) from a move *within* a screen
  (`/sessions/abc` → `/sessions/def`: keeps them, so the inspector does not close because the
  operator clicked the next row). Percent-encoding and `?`-in-hash parsing live here and nowhere else.
- **`src/routing/useAppLocation.ts`** — a **module-level store** over `window.location.hash`
  (`subscribe`/`getSnapshot` for `useSyncExternalStore`, listening on `hashchange`), exposing
  `useAppLocation(): { location, navigate(to, { replace }), setParams(patch, { replace }) }`.
  Module-level rather than a React context so a nested screen (and a Cypress component test that
  mounts one screen directly, with no router above it) shares the same source of truth without
  prop-drilling. `navigate` pushes via `window.location.hash = …` and replaces via
  `history.replaceState`; both notify subscribers synchronously so React state and the address bar
  never disagree within a tick.

`onNavigate` props stay — they are the screen-change seam the shell already uses and every existing
test stubs — but they are now thin wrappers over `navigate`.

### Route grammar

`src/routing/appRoutes.ts` gains `SESSIONS_NEW_ROUTE` (`/sessions/new`, with `new` reserved so
`parseSessionsDrawerSessionId` returns `null` for it), `sessionsDrawerAddAgentPath(id)` /
`parseSessionsDrawerAddAgent(path)`, `tasksPathForTask(id)` / `parseTaskId(path)`, and the param
name constants (`PARAM_HOST`, `PARAM_INSPECTOR`, `PARAM_FULL`, `PARAM_CODE`, `PARAM_CHANNEL`,
`PARAM_PROJECT`, `PARAM_PARTICIPANT`, `PARAM_SERVICE`, `PARAM_METHOD`) plus
`isInspectorTabName(value)` so an unknown `inspector=` value degrades to closed instead of
rendering a blank panel.

### Screens

- **`src/index.tsx`** — `usePathname` deleted; `App` reads `useAppLocation()` and dispatches on
  `location.path`. `#/` canonicalises to `#/sessions` with **replace**.
- **`src/routing/selectedHost.ts`** (new) — `resolveSelectedDaemonInstanceId` moves here, out of the
  `.tsx` provider, and gains a `urlInstanceId` argument at the top of the precedence chain (rules
  2–5 unchanged, including the "empty `daemons` is *no information yet*" invariant). The move is
  what makes it unit-testable: importing it from `rpc/selectedDaemon.tsx` pulls in React's JSX
  runtime, which is why the existing `src/rpc/selectedDaemon.test.ts` **cannot run under `bun test`
  today** — and, being outside every `bun test` path in `package.json`, nobody noticed. Those five
  pre-existing cases move to `src/routing/selectedHost.test.ts` alongside the new ones;
  `selectedDaemon.tsx` re-exports the function so existing importers are unaffected.
- **`src/rpc/selectedDaemon.tsx`** — reads `?host=` and feeds it to the resolver. `selectDaemon`
  writes `?host=` (push) **and** `sessionStorage`; a resolution that had no `host` in the URL writes
  one back with **replace**. A host change navigates to the current screen root, dropping the
  session sub-selection — which the existing `key={selectedInstanceId}` remount already invalidates.
- **`src/components/sessions/SessionsDrawerScreen.tsx`** — `selectedSessionId` is **derived from
  `location`**, not `useState`; `handleSelectSession` navigates. The one-shot `deepLinkActivatedRef`
  gate is replaced by a per-session-id activation guard so an inbound hash change (Back, pasted
  link) activates the newly named session, and a repeated render of the same id does not re-connect.
  `mode === "creating"` becomes `#/sessions/new`; `handleSessionCreated`'s direct
  `window.location.hash = …` write becomes a `navigate`. Inspector state maps to
  `inspector`/`full` — **push** on a user toggle/tab click, **replace** on the auto-open/auto-close
  driven by the attachment effect.
- **`src/components/sessions/SessionMainPane.tsx`** — `codeOpen` ⇄ `code=1`;
  `peerCreateInitialValues` presence ⇄ `#/sessions/:id/add-agent` (the captured values are still
  derived from the selected session at click time; the URL carries only the *mode*, and the
  submit-in-flight `peerCreateCaptureRef` behaviour is untouched).
- **`src/components/sessions/SessionInspectorDrawer.tsx`** — the internal `tab` `useState` becomes
  controlled `value`/`onChange` props supplied by the screen.
- **`src/components/tasks/TasksDrawerScreen.tsx`** / **`TaskOutputPane.tsx`** — `selectedTaskId` ⇄
  `#/tasks/:taskId`; `activeChannelId` ⇄ `channel` (the existing "fall back to the first channel
  when the id is unknown" rule is kept and now also covers an unknown `channel` param).
- **`src/components/worktrees/WorktreesAppPage.tsx`** — `projectId` ⇄ `project`. The existing
  "reset to the first project when the current one is no longer listed" effect writes the URL
  with **replace**. `daemonId` stays local state — it is a create-worktree form field, not a
  destination.
- **`src/rpc-playground/RpcPlaygroundAppPage.tsx`** / **`RpcPlaygroundScreen.tsx`** —
  `selectedService` / `selectedMethod` (Screen) and `selectedParticipantId` (AppPage) ⇄ params.
  `requestJson`, `editorMode`, `result`, `expanded` stay local (draft input and results, per the
  PRD non-goals). Acceptance coverage is at the Screen level (`service` / `method`); the
  AppPage-owned `participant` param has **no acceptance test** — the AppPage's harness is
  `cy.intercept`-driven around LiveKit reflection, and standing that up is disproportionate here.
  Covered by the shared `useAppLocation` helper's unit tests instead.

### Deliberately not in the URL

Draft form input (create-session fields, add-target forms, playground request JSON), confirmation
dialogs (delete confirm, VNC passphrase, branch conflict), the drawer open/closed flag (a
responsive layout concern that already defaults by viewport), and selection-mode / bulk-delete
ticks (a transient bulk operation).

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] `src/routing/appLocation.ts` — `AppLocation`, `parseAppLocation`, `formatAppLocation`,
      `withParams`, `withPath`
- [x] `src/routing/useAppLocation.ts` — module-level hash store + `useAppLocation()`
- [x] `src/routing/appRoutes.ts` — `/sessions/new`, `/sessions/:id/add-agent`, `/tasks/:id`,
      param-name constants, `isInspectorTabName`
- [x] `src/index.tsx` — replace `usePathname` with `useAppLocation`; canonicalise `#/`
- [x] `src/routing/selectedHost.ts` — extract `resolveSelectedDaemonInstanceId`, add `urlInstanceId`
- [x] `src/rpc/selectedDaemon.tsx` — read `?host=`, write-back on select, sub-selection drop;
      re-export the moved resolver; retire `src/rpc/selectedDaemon.test.ts` into
      `src/routing/selectedHost.test.ts`
- [x] `src/components/sessions/SessionsDrawerScreen.tsx` — selection, create mode, inspector state
- [x] `src/components/sessions/SessionInspectorDrawer.tsx` — controlled tab
- [x] `src/components/sessions/SessionMainPane.tsx` — Code pane, Add-agent mode
- [x] `src/components/tasks/TasksDrawerScreen.tsx` + `TaskOutputPane.tsx` — task + channel
- [x] `src/components/worktrees/WorktreesAppPage.tsx` — project filter
- [x] `src/rpc-playground/RpcPlaygroundAppPage.tsx` + `RpcPlaygroundScreen.tsx` — playground selection

## Acceptance tests

Written and confirmed failing (38 tests; 36 fail on missing functionality, 2 pass today as
regression guards for behaviour the refactor must preserve — marked below).

- [x] `cypress/component/SessionUrlStateAcceptance.cy.tsx` — 10 (9 fail; *"a `#/sessions/:id` deep
      link selects that session on load"* passes today — guard)
- [x] `cypress/component/SessionInspectorUrlStateAcceptance.cy.tsx` — 9 (9 fail)
- [x] `cypress/component/TaskUrlStateAcceptance.cy.tsx` — 6 (6 fail)
- [x] `cypress/component/SelectedHostUrlStateAcceptance.cy.tsx` — 6 (5 fail; *"falls back to the
      stored host when the URL names a daemon that is not in the room"* passes today — guard for
      precedence rules 2–5)
- [x] `cypress/component/WorktreesUrlStateAcceptance.cy.tsx` — 4 (4 fail)
- [x] `cypress/component/RpcPlaygroundUrlStateAcceptance.cy.tsx` — 3 (3 fail)

Support added: `cypress/support/pages/appLocationPage.ts` (the only place the specs touch
`window.location` / `history`), `worktreesPage.projectSelect`/`chooseProject`,
`sessionsDrawerPage.expectSelected`/`expectNotSelected`/`expectInspectorState` +
`inspectorWorktreeTab`/`inspectorFilesTab`, `TEST_IDS.worktreesProjectSelect`, and the shared hash
reset in `cypress/support/component.ts`.

## Unit tests

Written and confirmed failing (42 tests; every file errors on the missing module or export).

- [x] `src/routing/appLocation.test.ts` — 19: parse/format round-trip, param patching,
      screen-change vs within-screen path change
- [x] `src/routing/appRoutes.test.ts` (extended) — +16: `/sessions/new` reservation, add-agent,
      task id, inspector tab-name validation
- [x] `src/routing/selectedHost.test.ts` — 7: `resolveSelectedDaemonInstanceId` URL precedence

## Regression surface

Every existing spec that mounts a screen and asserts on selection state runs against the new
derived-from-URL selection. The ones that touch the hash or the selection directly:
`SessionsDrawerUnknownDeepLinkAcceptance`, `CreateSessionAutoClosesDrawer`,
`FastSessionChangeAcceptance`, `DaemonSelectionSurvivesRoomDisconnectAcceptance`,
`DaemonChangeReloadsScreenAcceptance`, `PrStackViewRoutingAcceptance`,
`WorkflowChatViewRoutingAcceptance`, `SessionChildTabsAcceptance`. A shared
`beforeEach` hash reset is added to `cypress/support/component.ts` so a hash left behind by one
spec cannot select a session in the next.

## Validation

- [x] `bun test src/routing` — **84/84** across `appLocation`, `appRoutes`, `selectedHost`
- [x] `bun run cypress:component` — new specs **38/38**
- [ ] ⚠️ **Full Cypress suite: 100 of 152 specs verified, 0 failures. The remaining 52 are not yet
      run.** The run was interrupted: a second worktree on this machine runs Cypress component tests
      concurrently and both bind Vite's default port 5173, so specs began fetching the *other*
      checkout's `cypress/support/component.ts` (`Failed to fetch dynamically imported module`) —
      an environment collision, not a test failure. The remainder is queued behind that run.
      A `VITE_PORT` override in `vite.config.ts` would make concurrent worktree runs safe; noted in
      [TODO.md](../TODO.md) rather than folded into this changeset.
- [x] `bun run build` (vite) — clean
- [x] `cargo fmt --check` — clean. **No Rust files changed**, so the Rust gates carry no signal for
      this changeset; `cargo clippy`/`cargo test` were not re-run for it.
- [x] `tsc --noEmit` — no new errors (master carries 334; the one this branch briefly added, a
      wrong `InvokeResult` variant in its own spec, is fixed)

## Validation Results

### `/validate-changes`

Two defects found and fixed, both from the same root cause — *state that used to be scoped to a
component now lives in a URL that outlives it*:

1. **Session-pane params outlived their session.** Leaving a session (Back, or the not-found
   state's "Back to sessions") left `inspector=` / `full=` / `code=` on `#/sessions`, so the URL
   claimed an open inspector over an empty pane. The activation effect now clears all three when no
   session is selected (`replace` — it is cleanup, not a destination).
2. **The RPC Playground's draft request survived a URL-driven method change.** `selectMethod` reset
   `requestJson`/`result` inside the click handler, so Back or a pasted link swapped the method
   underneath an unchanged body — one method's request shown under another's name. The reset moved
   to an effect keyed on the selected call, which covers both paths.

Also reviewed and found sound: no unguarded loops between the URL and React state (every
normalisation writes a value that makes its own guard false); `typeof window` guards on all three
DOM entry points; listener add/remove balanced in `subscribe`; the "empty list is *not loaded yet*"
invariant now held by both the daemon and the project resolvers.

### `/validate-tests` (fluent-tests compliance)

1. **[critical] Vacuous-pass risk.** `SelectedHostUrlStateAcceptance` hardcoded
   `"tddy_selected_daemon"`. Had production renamed the key, the seed would silently no-op and
   *"URL beats stored"* would pass while testing nothing. `SELECTED_DAEMON_STORAGE_KEY` is now
   exported from `routing/selectedHost.ts` and imported by the spec.
2. **[warning] Raw selectors in a test body.** `RpcPlaygroundUrlStateAcceptance` addressed
   `rpc-request-editor` / `rpc-method-…` inline. Added `cypress/support/pages/rpcPlaygroundPage.ts`
   and three `TEST_IDS` entries; the spec now speaks only in named helpers.
3. **[info]** Dead `.then(() => undefined)` tails removed from `appLocationPage`'s assertions.

Given/When/Then structure, one behaviour per test, `mountWithRpc` + `anInMemoryRpcBackend` (no
`cy.intercept`), and exact-equality assertions were already in place.

### `/validate-prod-ready`

Clean: no `TODO`/`FIXME` added (the three in touched files predate this branch), no `console.log`,
no `.only`/`.skip`, no test-only branches in production code, no secrets.

### `/analyze-clean-code`

1. **Duplicated rule.** "First path segment" was implemented twice — `screenOf` in `appLocation.ts`
   and `screenRootOf` in `selectedDaemon.tsx`. Consolidated into one exported
   `screenRootOf`, with four unit tests.
2. **Speculative API.** `setAppLocationPath` was exported with no consumer outside its module —
   now module-private.

Net effect of the whole changeset on the code it touched: `SessionsDrawerScreen` loses two of its
three selection code paths (the click handler and the deep-link effect collapse into one activation
effect), and `SessionMainPane` loses a `useState` plus the two effects that used to clear peer mode,
because the add-agent path already encodes it.
