# Changeset: optional-livekit-capability-gating

**Stack:** `optional-livekit` — node 4 of 7 (parents: `host-directory`, `session-connection`;
PR base `feature/optional-livekit/session-connection`)
PR: [#440](https://github.com/uppin/tddy-coder/pull/440)
PRD: [`2026-09-05-optional-livekit-capability-gating-prd.md`](2026-09-05-optional-livekit-capability-gating-prd.md)
Discovery: [`2026-09-05-optional-livekit-capability-gating-initial-discovery.md`](2026-09-05-optional-livekit-capability-gating-initial-discovery.md)

## State A

Inherited from nodes 1–3:

- `HostConnection` and `SessionConnection` both carry
  `capabilities: ReadonlySet<ConnectionCapability>` (`"rpc" | "media" | "presence"`).
- The host directory is source-merged; `room` has left `SelectedDaemonContextValue`; presence is
  reached through `useHostPresence(hostId)`, which already returns `null` without the capability.
- Session attachment has one connected status; a session resolved to the daemon advertises `{"rpc"}`.

**Nothing consults `capabilities` yet.** Every media and presence surface renders unconditionally:
`SessionVncTab`, `VncOverlay`, `SessionScreenSharingTab`, `ScreenSharingOverlay`,
`ParticipantVideoPreviewDialog`, `hooks/participantCameraVideo.ts`, `ParticipantList`,
`LiveKitRoomsPanel`, `LiveKitAppPage`, `RpcPlaygroundAppPage`'s participant list, and
`SessionsDrawerScreen`'s presence-derived cross-host reconciliation.

## State B

- `useHasCapability(connection, capability)` is the one predicate.
- Media-gated: VNC tab + overlay, screen-sharing tab + overlay, participant video preview, camera
  video hooks. Presence-gated: participant list, rooms panel, LiveKit app page and its nav entry,
  playground participant list.
- Gated surfaces are removed from navigation rather than disabled; where an entry point must remain
  for layout, it names the reason it is unavailable.
- `SessionsDrawerScreen` degrades honestly without presence: `ListSessions` only, and no claim of
  completeness.
- With a `{"rpc"}`-only connection selected, no `livekit-client` `Room` is constructed anywhere.

## Responsibility

- `useHasCapability` and the gating decisions for every surface listed above.
- Navigation entries for gated screens (`LiveKitAppPage`, nav menu, session tabs).
- The honest-degradation path in `SessionsDrawerScreen`'s cross-host reconciliation.
- Cypress acceptance coverage for a `{"rpc"}`-only connection across every affected screen, and
  regression coverage that a fully capable connection is unchanged.

## Boundaries

- Does **not** define `ConnectionCapability`, `HostConnection` or `SessionConnection` — nodes 1 and 3
  own those. This PR only reads `capabilities`.
- Does **not** change the host directory, its sources, or `useHostPresence`'s signature — node 2's.
- Does **not** change how a connection is opened, routed, cached or closed — node 3's.
- Does **not** merge the terminal components. Node 5 owns that; this PR does not touch
  `GhosttyTerminalLiveKit`, `GhosttyTerminalGrpc` or terminal selection.
- Does **not** add an IPC transport or register a desktop provider. Nodes 6 and 7.
- Does **not** delete any media surface, and does not change what any of them do when the capability
  *is* present. Gating only.
- Adds **no npm dependency**.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `host-directory` (#438) | source-merged directory, `room` removed from `SelectedDaemonContextValue`, `useHostPresence(hostId)` returning `null` without the `presence` capability, directory-level status | presence gating reads `useHostPresence`; the nav gate reads the selected host's connection | add a directory source; change `useHostPresence`'s signature; put `room` back on the context |
| `session-connection` (#439) | `SessionConnection` with `capabilities`, one connected `SessionAttachmentState`, capability-driven handshake overlay and terminal selection | `SessionRuntime`'s terminal-side media branch reads the session connection's capabilities, through the predicate | change `openSession`, the hint type, routing, the client cache, or terminal selection |

**Sequencing:** the stack is linear — `connection-model` → `host-directory` → `session-connection` →
this node — so both parents are ordinary ancestors of this PR's base and arrive through it. The
`stack-int/capability-gating` integration ref the plan originally called for is not needed and is not
used; `useHostPresence` and everything else node 2 owns is present at this branch's `HEAD`. This PR is
still only offered for merge once both parents have merged, which bottom-up landing gives for free.

### Which connection gates which surface

Settled during `/green`, because the plan's original answer did not survive contact with dormant
sessions. **The host connection is what gates the media surfaces**, not the session connection:

- `capabilitiesForHint` (`src/rpc/connections/sessionAttachment.ts:69`) derives a session's
  capabilities from whether its hint names a room, and whether there is a room is decided by how the
  *host* is reached. Host capability is therefore the upstream fact: a host reached without LiveKit
  can never hand out a room-backed session.
- A dormant session has no `SessionConnection` at all — `SessionsDrawerScreen` attaches only when
  `session.isActive`. Reading the session connection there answers `false` to a question that is
  actually *unanswerable*, which is not gating; it hid the VNC and screen-sharing tabs for every
  dormant session and broke 23 tests in four acceptance specs that this PR is required to leave
  passing.

The session connection is still read in exactly one place, `SessionRuntime`'s `carriesMedia` — that
one genuinely is session-scoped, and it predates this node.

### The order the two facts are read in, for every gated surface

Settled during `/green` — first for presence, then extended to media on the user's call, so both
halves of this PR answer the availability question identically.

No capability can be gated on the predicate alone. A common room that is **still joining** has not
yet produced a connection carrying anything — `LiveKitConnections` is bound to a `null` room until
`Room.connect()` resolves — and a join that **failed** produces no connection at all. A surface
asking the capability first would therefore announce "not available on this connection" for the
second or two every LiveKit page spends connecting, and then contradict itself. Both halves had a
concrete symptom:

- **Presence** — it would have replaced the ICE-failure reason that
  `CommonRoomConnectionVisibilityAcceptance` exists to pin (the 2026-08-13 `udoo` incident) with a
  capability verdict, which is the incident's own failure mode restated.
- **Media** — the inspector's tab strip rendered seven tabs on load and nine a second later,
  reflowing under the operator's cursor on every LiveKit page load.

`src/hooks/capabilityAvailability.ts` is the resulting order, in one place: `error` → `connecting` →
`unavailable` → `available`. Status answers for a capability that exists; the predicate answers
whether it exists at all. So the nav entry and the screen it points at cannot disagree — and a
failed join keeps both the entry and the tabs, because the reason it failed is what an operator
would go there to find.

**`src/hooks/useCapabilityAvailability.ts` is how a surface asks it.** Centralising the rule was
supposed to stop the surfaces drifting, but the drift surface had merely moved up a level: six
components still re-assembled the same three lines around it — resolve the connection, read the
common room out of the host directory, apply `useHasCapability`, default a missing source to
`idle`. Six copies of an argument list are six chances to pass the wrong capability, read a
different source, or default the missing status the other way. `useCapabilityAvailability(connection,
capability)` is now the only place any of that happens, `?? "idle"` included, and a call site states
only what is genuinely its own: which connection, and which capability. It takes a connection rather
than a host id because `SessionInspectorDrawer` is *handed* one as a prop and has no id to resolve
from — a hook keyed on an id would have left that site assembling the answer by hand, which is the
thing being removed.

`LiveKitRoomsPanel`, `LiveKitAppPage`, `DaemonNavMenu`, `SessionsDrawerScreen`,
`SessionInspectorDrawer` and `RpcPlaygroundAppPage` all read the hook. `ParticipantList` is the one
surface that calls the pure rule directly: it is presentational and is *told* which room's status it
is reporting on, so the screen above it owns that half.

Treating `connecting` as "keep it" is safe in the other direction too: a host that never joins a
room — the desktop build over IPC — reports `idle` and never `connecting`, so nothing appears there
only to vanish.

#### The status half is fleet-wide; the capability half is one host's

A known limitation, recorded rather than fixed, and **node 7 must revisit it**.

`useHostDirectorySource(LIVEKIT_SOURCE_ID)` is *one source for the whole page* — the common room
either joined or it did not. The capability is a property of one host's connection. So while the
common room is `connecting`, or permanently in `error`, the rule answers "not unavailable" for
**every** host, including one reached over a wire that provably cannot carry a track.

Scoping the status per host was investigated and does not work today:

- **A host's own `status` cannot supply it.** `LiveKitConnectionProvider.connectHost` returns `null`
  until it has a room (`rpc/connections/liveKit.tsx`), so during the very window the rule exists to
  cover there is no `HostConnection` to read a status — or a capability — off at all. That absence
  *is* the fact the common-room source is standing in for.
- **`HostDescriptor.sourceId` cannot supply it either.** It names which source *advertised* the host
  first, not which wire would carry the capability, and the two differ for the case that matters:
  the serving daemon is advertised by the `serving` source (`connected` from the first paint) and is
  simultaneously reachable over LiveKit. Reading the serving source's status would make the media
  tabs vanish and return on every LiveKit page load — exactly the reflow this rule was added to stop,
  and `SessionInspectorMediaCapabilityAcceptance`'s "keeps the media tabs while the common room is
  still being joined" would fail on it.

It is acceptable today because the fleet is single-wire: `LiveKitConnectionProvider` is the only
registered provider, so every `HostConnection` in existence answers the capability question
identically and a fleet-wide status cannot contradict a per-host one. The surfaces that stay visible
under `connecting`/`error` also explain themselves — the roster quotes the join failure, the rooms
panel says what it is waiting on, the LiveKit screen keeps its route.

It stops being acceptable at **node 7**, which registers the IPC provider and so creates the first
fleet where one host is reached over IPC and another over LiveKit. There, a common room stuck in
`error` would keep the media tabs on an IPC-reached host that can never serve a track. The fix
belongs in one place — `useCapabilityAvailability` — which is the other reason the hook exists;
what it needs is a way to ask "which source would carry `capability` for *this* host", which node 6
is the first node in a position to define.

| Surface | Gated on | Absent state |
|---|---|---|
| `ParticipantList` | host connection + common-room status | names the connection as the reason; the `idle` branch no longer claims "Connecting…" forever |
| `LiveKitRoomsPanel` — the panel | host connection + common-room status | removed; while the join is in flight or failed the panel keeps its place and says what it is waiting on, rather than dropping in under the roster once the join lands |
| `LiveKitRoomsPanel` — its `StreamLiveKitRooms` feed | host connection | never subscribed (the feed is a child component, so the hook does not run) — a different question from whether the panel applies, and deliberately a different gate |
| `LiveKitAppPage` | host connection + common-room status | the route stays reachable and explains itself, as a media deep link degrades to Details |
| `DaemonNavMenu`'s LiveKit entry | host connection + common-room status | removed from the menu |
| `RpcPlaygroundScreen`'s participant picker | host connection + common-room status (decided in `RpcPlaygroundAppPage`) | replaced by the reason there is nobody to address |
| `SessionsDrawerScreen` cross-host rows | host connection + common-room status | `ListSessions` rows plus a footnote naming what is out of view |
| `SessionInspectorDrawer`'s VNC + Screen Sharing tabs, panel dispatch and `?inspector=` fallback | host connection + common-room status | removed from the strip; a media tab named in the URL degrades to Details, and is honoured the moment the wire can serve it |

`ParticipantList`'s `data-room-status` reports the room's **real** status once the panel decides it
has a roster to render. Collapsing `idle` and `connecting` into one "Connecting…" branch used to make
that attribute true by construction; replacing that guard with `availability === "connecting"` left
`idle` falling through to a branch hard-coded to `connected`, so the DOM claimed a room was joined
that nothing had joined. The verdict `available` is about the *capability*, not about the join — a
host that never joins a room reaches it from `idle` — so the attribute renders `roomStatus` itself.

Two media reads deliberately stay on the bare predicate. `ParticipantList`'s camera column sits
*inside* the rendered roster, which only exists once presence has resolved — while the room joins
the panel shows "Connecting…", so there is no strip to reflow. And `SessionRuntime`'s `carriesMedia`
is session-scoped and predates this node; a session connection exists only once the session is
attached, which is not a common-room join.

Five props became **required** rather than defaulted — `ParticipantList.connection`,
`RpcPlaygroundScreen.presenceAvailable`, `SessionDrawer.crossHostSessionsVisible`,
`SessionInspectorDrawer.hostConnection` and `SessionMainPane.host` — following
`InspectorTabs.mediaAvailable` from the media milestone. A default would mean "unknown ⇒ show it",
which is the silence this node exists to remove: a list that has lost rows looks exactly like a list
that never had them.

The last two defaulted the *other* way, `= null`, which is worse in the same manner: `null` means "no
media", so a call site that simply forgot the prop lost the VNC and Screen Sharing tabs silently and looked
exactly like a host that cannot carry a track. Eleven specs now name the host they mount with —
`null` in every one, which is what they were already getting, so no behaviour and no assertion moved;
what changed is that the answer is stated rather than inherited.

`useHostPresence` now spells its check `useHasCapability(connection, "presence")`. Its signature is
untouched — node 2 owns that.

## Draft PR contract

Lands first, so node 5 and node 7 can branch off a real ref:

1. `src/rpc/connections/useHasCapability.ts` — the predicate, with its final signature.
2. Failing Cypress acceptance tests, one per gated surface, asserting it is absent on a
   `{"rpc"}`-only connection and present on a fully capable one:
   VNC tab, screen-sharing tab, participant video preview, participant list, rooms panel,
   LiveKit app page + nav entry, playground participant list.
3. A failing acceptance test asserting the sessions drawer shows `ListSessions` rows and no
   completeness claim when presence is absent.
4. A failing test asserting no `livekit-client` `Room` is constructed under a `{"rpc"}`-only
   connection on any screen.

Implementation lands in the same PR under `/green`. **Not a merge candidate on the contract alone.**

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests — `cypress/component/CapabilityGatingAcceptance.cy.tsx`
- [x] Run acceptance tests (verify they fail) — 5/5 on `useHasCapability`
- [x] USER REVIEW — acceptance tests — waived 2026-09-05 (run wave 2 straight through)
- [x] TDD Red — write failing unit/integration tests — `src/rpc/connections/useHasCapability.test.ts`
- [ ] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap`

## Verification

### Baseline after rebasing onto both parents

`bun run --filter tddy-web test:unit` — 971 pass, **11 fail**. All 11 are inherited red from the
parents' contract commits: 6 from `connection-model` (#437) and 5 from `session-connection` (#439).
None is this node's to fix. This node adds 6 more.

The full Cypress component sweep ran once at node 1 (207 specs, 1213/1214; the one failure
pre-existing in `SelectedHostUrlStateAcceptance.cy.tsx`). One full sweep runs at the Step 8
completion gate.

### Red status at the contract commit

| Suite | Result |
|---|---|
| `src/rpc/connections/useHasCapability.test.ts` | **6 tests, 6 failing** |
| `cypress/component/CapabilityGatingAcceptance.cy.tsx` | **5 tests, 5 failing** |

Every failure is on this node's own `TODO(capability-gating)` body.

**One test was caught passing spuriously and fixed.** `removes the participant panel` asserted only
`should("not.exist")`, which a component that *throws* satisfies just as well as one that correctly
hides the panel — so it went green against an unimplemented predicate. It now asserts the tab strip
that does render first, so absence means absence. Worth remembering for the rest of this node's
`/green`: every `not.exist` in a gating test needs a positive assertion beside it.

### Green status — presence surfaces

| Suite | Result |
|---|---|
| `bun run --filter tddy-web test:unit` | **1028 pass, 0 fail** (baseline 1024 + `src/hooks/capabilityAvailability.test.ts`) |
| `cypress/component/PresenceCapabilityGatingAcceptance.cy.tsx` | **14 pass** — both directions for the roster, the rooms panel and its feed, the screen, the nav entry, the playground picker and the drawer footnote |
| `cypress/component/SessionInspectorMediaCapabilityAcceptance.cy.tsx` | **12 pass** — the original 10, plus the tabs surviving a join in flight and staying absent on a host that joins no room |
| 47 existing spec files re-run (~380 tests) | **all pass**, no assertion changed |

Three of the new tests were checked for vacuity by mutation. Reading the capability without the
status rule fails "keeps the LiveKit entry while the common room is still being joined", "says
nothing about other hosts while the common room is still being joined" and "keeps the media tabs in
the strip while the common room is still being joined"; gating `LiveKitAppPage` the same way fails
`CommonRoomConnectionVisibilityAcceptance`'s incident regression.

### Green status — the validation pass

One hook (`src/hooks/useCapabilityAvailability.ts`) now composes the rule for all six gated
surfaces — the four that were re-assembling its two arguments by hand, and the two that were reading
the bare predicate. Four inconsistencies found by validation are closed: the playground picker and
the rooms panel ask the rule rather than the predicate, `SessionInspectorDrawer.hostConnection` and
`SessionMainPane.host` became required, and the roster stopped stamping `connected` on a room that is
`idle`. The fleet-wide status limitation above is recorded, not fixed.

| Suite | Result |
|---|---|
| `bun run --filter tddy-web test:unit` | **1046 pass, 0 fail** (baseline 1044 + the two missing `capabilityAvailability` truth-table rows) |
| `cypress/component/PresenceCapabilityGatingAcceptance.cy.tsx` | **16 pass** — the 14, plus the rooms panel holding its place unsubscribed through a join and the playground picker surviving one |
| 28 further specs re-run (183 tests) | **all pass**, no assertion changed |

The 28: `SessionInspectorMediaCapabilityAcceptance` (12), `ParticipantVideoCapabilityAcceptance` (4),
`CapabilityGatingAcceptance` (5), `ParticipantList` (12), `LiveKitRoomsPanelAcceptance` (26),
`LiveKitScreenAcceptance` (1), `CommonRoomConnectionVisibilityAcceptance` (4),
`RpcPlaygroundScreen` (8), `RpcPlaygroundUrlStateAcceptance` (3), `UnifiedLayoutAcceptance` (3),
`SessionInspectorVncAcceptance` (5), `SessionVncTargetRowsAcceptance` (5),
`SessionInspectorScreenSharingAcceptance` (8), `SessionScreenSharingTargetRowsAcceptance` (5),
`SessionInspectorUrlStateAcceptance` (9), `SessionInspectorAcceptance` (14),
`SessionsDrawerCrossHostAcceptance` (8), `SessionInspectorFilesTab` (2),
`SessionInspectorSplitRoster` (3), `InactiveSessionActivitiesAcceptance` (18),
`PrStackChatSilentFailureAcceptance` (5), `PrStackPresenterRoomAcceptance` (2),
`SessionInactiveInspectorOverlay` (6), `SessionMainPaneLiveKitTerminal` (5),
`SessionMainPaneTerminalControl` (1), `SessionMainPaneTraffic` (7),
`SessionMainPaneUnscopedSession` (1), `WorkflowChatPresenterRoomAcceptance` (1).

### Commands

```bash
./dev bun run --filter tddy-web test:unit
./dev bun run --filter tddy-web cypress:component
```

Installs via `bun run local-registry-install` — the public npm registry is unavailable.

## Successor PRs

- [`2026-09-05-optional-livekit-terminal-convergence.md`](2026-09-05-optional-livekit-terminal-convergence.md)
  — branch `feature/optional-livekit/terminal-convergence`
- [`2026-09-05-optional-livekit-desktop-ipc-host.md`](2026-09-05-optional-livekit-desktop-ipc-host.md)
  — branch `feature/optional-livekit/desktop-ipc-host`
