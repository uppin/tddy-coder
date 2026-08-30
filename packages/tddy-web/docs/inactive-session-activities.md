# Inactive session activities (main-pane base view)

Technical reference for the base-view rule in **`packages/tddy-web/src/components/sessions/sessionBaseView.ts`**, the pane in **`SessionActivitiesPane.tsx`**, and the attach bookkeeping in **`SessionsDrawerScreen.tsx`** that lets a resumed session return to its terminal.

Product reference: [docs/ft/web/inactive-session-activities.md](../../../docs/ft/web/inactive-session-activities.md).

## Base view selection

**`sessionBaseViewMode(session, hasWorkflowView) → "workflow" | "activities" | "terminal"`** — pure, so the rule is pinned without React.

- A **workflow view wins outright**. `PrStackScreen` renders planned PRs from persisted stack state and `WorkflowChatScreen` is already a transcript; both stay meaningful when the session is dormant, so the Activities view replaces exactly one surface — the terminal.
- Otherwise a **dormant** session (`connectionStatusForSession === "disconnected"`) yields `"activities"`.
- The workflow branch short-circuits **ahead of** the null-session check; `sessionBaseViewMode(null, true)` is `"workflow"`. Pinned by a test so it is not left to reading order.

**`canResumeSession(session)`** is the same dormancy predicate, used alone for the top-bar Resume. Keying Resume on liveness rather than on the base view is what gives a dormant `pr-stack` orchestrator the button in the same position as a dormant terminal session.

`SessionMainPane` composes these into a four-arm base view: `customView` → activities-over-runtime-layer → runtime layer → connected placeholder → disconnected placeholder. The runtime layer is extracted into one variable because a dormant session renders the Activities view **over** it, not instead of it — the runtimes stay mounted (background streaming preserved, a later resume is instant) with `focused={!dormant && …}`, so nothing foregrounds a stale terminal or its claim-terminal CTA.

## Liveness semantics

`connectionStatusForSession` returns `"needs-input"` only when **`isActive && pendingElicitation`**. The flag is persisted and is not cleared when a process dies, so a session that died mid-elicitation used to read as live — which under the rules above left it with neither the Activities view nor a Resume button. The predicate change also reaches the live/remaining partition (`sessionStackGroups`) and sorting; a dead session with a stale flag now reads as disconnected everywhere. The drawer status **dot** is no longer derived from `connectionStatusForSession` — it is `sessionIndicatorFor` (`src/lib/sessionIndicator.ts`), which keeps the same liveness-first rule and adds the notification-driven states (see [session-notifications](../../../docs/ft/daemon/session-notifications.md)).

## Transcript pane

`SessionActivitiesPane` renders the same recorded ACP transcript as `AgentActivityOverlay` — `useAcpReplay` → `AgentChatView room={null} readOnly` → `AgentActivityDetailDialog` — with two differences:

- The snapshot loads **eagerly** on mount. The overlay defers because it is a popover the operator may never open; this *is* the view, so deferring would only add a blank frame. Both share the `agentActivityRegistry` cache, so a transcript read in one costs nothing in the other.
- The overlay's icon is suppressed while this pane renders (`baseViewMode !== "activities"` in `SessionMainPane`), so a pane never carries two copies of the same transcript. The overlay remains the only route to the transcript for an active session and for a dormant session showing a workflow view.

**`countLoaded`** (on `agentActivityRegistry`, exposed through `useAcpReplay`) is set on the first `COUNT_THEN_LIVE` frame **whatever value it carries**. `hasActivity` is `count > 0` starting from 0, so without this flag the pane asserts *"This session recorded no activity"* during the window before the count feed answers — and identically when the feed **fails**, since `useAcpReplay` swallows a stream error. The pane renders nothing until the count is known, so a pending or failed read never becomes a false claim.

## Attach bookkeeping

Two effects can issue `ConnectSession` for a selected session: the activation effect (runs once per selection) and a liveness effect (attaches a selected session that comes alive — from the top-bar Resume, or resumed elsewhere and seen on the next list poll). Without the second, the pane would keep showing the transcript until the session was re-selected.

They share **`attachClaimRef: { sessionId, listGeneration }`**. The claim means *an attach has been taken for the current live epoch*, and is released once a **later** list snapshot reports the session dormant:

- Guarding release on "do we still hold an attachment for this session" **deadlocks** — nothing resets a stale `connected-livekit` attachment when a session dies, so the claim is never released and a second Resume within one selection silently does nothing.
- The generation counter is incremented **during render**, not in an effect, so a claim taken from a click handler records the snapshot the operator was actually looking at. The snapshot a resume was made under still reports dormant (hold); a later one reporting dormant is real evidence of death (release).

`handleResume` claims the attach on success and releases it if the resume rejects — otherwise `resumeSession` (which already drives the attachment to `connected-livekit`) and the liveness effect both fire, minting a fresh browser identity and forcing a terminal reconnect plus a second `ClaimTerminalControl`.

## Inspector

`defaultInspectorOpen()` takes no argument and returns `false`: selecting a session never opens the drawer. Only a deep-linked `?inspector=<tab>` (first activation) or an attach **error** opens it. Docking is removed — `isInspectorDocked`, the `docked` prop and the `data-docked` attribute are gone, and the drawer is always an overlay, because the pane behind it is no longer empty.

## Testing constraint

The Cypress **component** harness loads no stylesheet: `cypress/support/component-index.html` links none and `cypress/support/component.ts` imports none. Every Tailwind class is inert and every element measures the full viewport width, so **layout assertions are meaningless there** — an attempt to pin the overlay's ~360px footprint fails with `expected 1280 to be below 1280` regardless of the rendered layout. Since `data-docked` was the only harness-observable overlay/docked discriminator, the removal of docking is pinned only indirectly, by the base view surviving behind an open drawer. See [docs/dev/TODO.md](../../../docs/dev/TODO.md) before re-attempting geometry assertions.
