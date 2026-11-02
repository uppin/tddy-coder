# Inactive session activities (main-pane base view)

Technical reference for the base-view rule in **`packages/tddy-web/src/components/sessions/sessionBaseView.ts`**, the stopped Agent tab in **`DormantAgentTerminal.tsx`**, the transcript pane in **`SessionActivitiesPane.tsx`** (the inspector's Activity tab), and the attach bookkeeping and resume hint in **`SessionsDrawerScreen.tsx`** that let a resumed session return to its terminal.

Product reference: [docs/ft/web/inactive-session-activities.md](../../../docs/ft/web/inactive-session-activities.md).

## Base view selection

**`sessionBaseViewMode(session, hasWorkflowView) → "workflow" | "activities" | "terminal"`** — pure, so the rule is pinned without React.

- A **workflow view wins outright**. `PrStackScreen` renders planned PRs from persisted stack state and `WorkflowChatScreen` is already a transcript; both stay meaningful when the session is dormant, so the Activities view replaces exactly one surface — the terminal.
- Otherwise a **dormant** session (`connectionStatusForSession === "disconnected"`) yields `"activities"`.
- The workflow branch short-circuits **ahead of** the null-session check; `sessionBaseViewMode(null, true)` is `"workflow"`. Pinned by a test so it is not left to reading order.

**`canResumeSession(session)`** is the same dormancy predicate, used alone for the top-bar Resume. Keying Resume on liveness rather than on the base view is what gives a dormant `pr-stack` orchestrator the button in the same position as a dormant terminal session.

`SessionMainPane` composes these into a four-arm base view: `customView` → stopped-strip-over-runtime-layer → runtime layer → connected placeholder → disconnected placeholder. The runtime layer is extracted into one variable because a dormant session renders its own tab strip **over** it, not instead of it — the runtimes stay mounted (background streaming preserved, a later resume is instant) with `focused={!dormant && …}`, so nothing foregrounds a stale terminal or its claim-terminal CTA.

### The stopped session's strip

`DormantSessionRuntime.tsx` is the tab strip of a session with no live connection. It reuses `SessionTerminalTabs`, `useSessionTerminals` and `GrpcSessionTerminal` against the owning daemon's `TerminalSessionService` client — none of which needs the agent's connection — and claims the session's control lease on attach (`useTerminalControl`) as a live strip does. `SessionMainPane` keys it by session id, so one session's terminal list and lease never carry into another's.

- **The Agent tab is a render slot** (`agentTab: React.ReactNode`); `SessionMainPane` fills it with `DormantAgentTerminal`. The strip never decides what a stopped Agent tab shows.
- **What a keystroke would reach** (`focusedTerminalId`) is a bash or harness tab with no conversation over it — never the Agent tab, which is read-only. Only that tab gets the mobile shortcut drawer, the claim toast and grey-out (`TerminalControlLayer`, rendered only while another screen holds the lease), and the Files-tab Insert.
- **Insert** reaches a live runtime through the session runtime registry. A stopped session has none, so the strip hands `SessionMainPane` a function (`onRegisterInsert`) that types into its focused bash tab, and `SessionMainPane` routes the inspector's `onInsertPathIntoTerminal` to it while the stopped strip shows.
- **Full screen** uses the same `useSessionPaneFullscreen` and `PaneFullscreenExit` as the live strip. The shared pane chrome — `PaneFullscreenExit`, `TerminalControlLayer` — lives in `SessionPaneChrome.tsx`, so the two strips cannot drift.
- **Agent-conversation bodies are not rendered by the strip** — see [session-agent-conversation.md](session-agent-conversation.md#1-conversation-bodies-are-owned-by-neither-strip).

### The stopped Agent tab

`DormantAgentTerminal({ sessionId, sessionToken, terminalClient, onResume })` is a `GhosttyTerminal` with no `onData` sink, so a keystroke reaches nothing. It uses `HISTORY_THEME` and `HISTORY_FILTER`, exported from `GhosttyTerminalSession.tsx` rather than copied, so the stopped tab and "View history" cannot drift apart.

- **`useRecordedOutput`** waits for the emulator's `onReady` (writes before open are lost), then drives a `TerminalHistoryForwardLoader(null)` over `createForwardHistoryFetcher` for `AGENT_TERMINAL_ID` until `done`. With no live stream to anchor on, the fill runs to the daemon's `atEnd`. `NOT_FOUND`, or a recording that wrote nothing, sets *no recording* ("No recorded output"); any other failure is shown through `rpcFailureText`. A missing terminal client is its own message, not an empty recording.
- **`terminal-buffer-text`** mirrors the buffer the way `GhosttyTerminalSession` does, so the terminal specs read it the same way.
- **`preventFocusOnTap={isMobile}`**: on a touch device a tap does not focus the read-only terminal, so the keyboard stays down; a pointer device keeps the default focus.
- **The toast** is `Toast` (`src/components/ui/toast.tsx`): `{ message, action?: { label, onClick, busy? }, error?, testId }`, `role="status"`, its error `role="alert"` through `InlineError`, absolutely positioned like `TerminalControlOverlay`. There is no toast queue; the caller owns placement. `useResumeAction` keeps the action busy while `onResume` runs and shows its rejection's text.

## Resume

`useSessionsDrawerPaneActions` owns one resume action, **`resume(sessionId): Promise<void>`**, which resolves once the session is attached and rejects with why it did not land (host not connected, or `ResumeSession`'s failure). `SessionMainPane` takes it as `onResumeSession` and hands it to the stopped Agent tab's toast, which shows the rejection. `handleResume` — the top bar and the inspector's Details tab — is `resume` with the rejection dropped, because that failure is already the pane's error (`paneError`). For that, `useSessionAttachment.resumeSession` resolves the failure's text (or `null`) instead of a boolean; `connectSession` keeps its boolean.

### The resumed-session hint

Liveness only refreshes on the drawer's 2s `ListSessions` poll, so after a successful resume the stopped strip would linger until the next poll. **`resumedSessions.ts`** is pure state over a `ReadonlySet<string>` of session ids: `markResumed` after a resume lands, `afterListing` drops every id a listing reports live, `isLive` is `isActive || marked`, and `withResumedLiveness(resumed, session)` returns the session with `isActive: true` while it is marked (the same reference otherwise). **`useResumedSessionLiveness(sortedSessions, selectedSession)`** holds the set, settles it on every list snapshot and returns `liveSelectedSession` plus `markSessionResumed`, which `resume` calls on success.

Only `SessionMainPane` reads `liveSelectedSession`. The activation and liveness effects keep reading the listing, so the attach bookkeeping below is unchanged: `ResumeSession` itself drives the attachment to `connected`, and the claim stops the liveness effect from attaching a second time.

## Liveness semantics

`connectionStatusForSession` returns `"needs-input"` only when **`isActive && pendingElicitation`**. The flag is persisted and is not cleared when a process dies, so a session that died mid-elicitation used to read as live — which under the rules above left it with neither the Activities view nor a Resume button. The predicate change also reaches the live/remaining partition (`sessionStackGroups`) and sorting; a dead session with a stale flag now reads as disconnected everywhere. The drawer status **dot** is no longer derived from `connectionStatusForSession` — it is `sessionIndicatorFor` (`src/lib/sessionIndicator.ts`), which keeps the same liveness-first rule and adds the notification-driven states (see [session-notifications](../../../docs/ft/daemon/session-notifications.md)).

## Transcript pane

`SessionActivitiesPane` renders the same recorded ACP transcript as `AgentActivityOverlay` — `useAcpReplay` → `AgentChatView room={null} readOnly` → `AgentActivityDetailDialog` — with two differences:

- It is mounted by **`SessionInspectorActivityTab`**, the inspector's Activity tab, with the drawer's `activityClient` (`buildSessionActivityClient?.() ?? activityClient`, from `SessionMainPane`). `activity` is in `INSPECTOR_TAB_NAMES`; `DORMANT_INSPECTOR_TAB_NAMES` names it as the stopped-only tab, and `SessionInspectorDrawer` offers it (`InspectorTabs` `activityAvailable`) only while `canResumeSession(session)`. A request for it on a live session degrades to Details without being normalised out of the URL, exactly as a media tab does on a host without tracks.
- The snapshot loads **eagerly** on mount. The overlay defers because it is a popover the operator may never open; here the operator opened the tab for it. Both share the `agentActivityRegistry` cache, so a transcript read in one costs nothing in the other.
- The overlay's icon is suppressed while the stopped strip shows (`baseViewMode !== "activities"` in `SessionMainPane`), so a pane never carries two copies of the same transcript; the Activity tab is that session's transcript route. The overlay remains the only route to the transcript for an active session and for a dormant session showing a workflow view.

**`countLoaded`** (on `agentActivityRegistry`, exposed through `useAcpReplay`) is set on the first `COUNT_THEN_LIVE` frame **whatever value it carries**. `hasActivity` is `count > 0` starting from 0, so without this flag the pane asserts *"This session recorded no activity"* during the window before the count feed answers — and identically when the feed **fails**, since `useAcpReplay` swallows a stream error. The pane renders nothing until the count is known, so a pending or failed read never becomes a false claim.

## Attach bookkeeping

Two effects can issue `ConnectSession` for a selected session: the activation effect (runs once per selection) and a liveness effect (attaches a selected session that comes alive — resumed elsewhere and seen on the next list poll). Without the second, the pane would keep showing the stopped strip until the session was re-selected.

They share **`attachClaimRef: { sessionId, listGeneration }`**. The claim means *an attach has been taken for the current live epoch*, and is released once a **later** list snapshot reports the session dormant:

- Guarding release on "do we still hold an attachment for this session" **deadlocks** — nothing resets a stale connected attachment when a session dies, so the claim is never released and a second Resume within one selection silently does nothing.
- The generation counter is incremented **during render**, not in an effect, so a claim taken from a click handler records the snapshot the operator was actually looking at. The snapshot a resume was made under still reports dormant (hold); a later one reporting dormant is real evidence of death (release).

`resume` claims the attach before `ResumeSession` and releases it if the resume fails — otherwise `resumeSession` (which already drives the attachment to `connected`) and the liveness effect both fire, minting a fresh browser identity and forcing a terminal reconnect plus a second `ClaimTerminalControl`.

## Inspector

`defaultInspectorOpen()` takes no argument and returns `false`: selecting a session never opens the drawer. Only a deep-linked `?inspector=<tab>` (first activation) or an attach **error** opens it. Docking is removed — `isInspectorDocked`, the `docked` prop and the `data-docked` attribute are gone, and the drawer is always an overlay, because the pane behind it is no longer empty.

## Testing constraint

The Cypress **component** harness loads no stylesheet: `cypress/support/component-index.html` links none and `cypress/support/component.ts` imports none. Every Tailwind class is inert and every element measures the full viewport width, so **layout assertions are meaningless there** — an attempt to pin the overlay's ~360px footprint fails with `expected 1280 to be below 1280` regardless of the rendered layout. Since `data-docked` was the only harness-observable overlay/docked discriminator, the removal of docking is pinned only indirectly, by the base view surviving behind an open drawer. See [docs/dev/TODO.md](../../../docs/dev/TODO.md) before re-attempting geometry assertions.
