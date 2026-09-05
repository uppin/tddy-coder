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
whether it exists at all. `ParticipantList`, `LiveKitAppPage`, `DaemonNavMenu`,
`SessionsDrawerScreen` and `SessionInspectorDrawer` all read it, so the nav entry and the screen it
points at cannot disagree — and a failed join keeps both the entry and the tabs, because the reason
it failed is what an operator would go there to find.

Treating `connecting` as "keep it" is safe in the other direction too: a host that never joins a
room — the desktop build over IPC — reports `idle` and never `connecting`, so nothing appears there
only to vanish.

| Surface | Gated on | Absent state |
|---|---|---|
| `ParticipantList` | host connection + common-room status | names the connection as the reason; the `idle` branch no longer claims "Connecting…" forever |
| `LiveKitRoomsPanel` | host connection | removed, and its `StreamLiveKitRooms` feed never subscribed (the panel body is a child component, so the hook does not run) |
| `LiveKitAppPage` | host connection + common-room status | the route stays reachable and explains itself, as a media deep link degrades to Details |
| `DaemonNavMenu`'s LiveKit entry | host connection + common-room status | removed from the menu |
| `RpcPlaygroundScreen`'s participant picker | host connection (decided in `RpcPlaygroundAppPage`) | replaced by the reason there is nobody to address |
| `SessionsDrawerScreen` cross-host rows | host connection + common-room status | `ListSessions` rows plus a footnote naming what is out of view |
| `SessionInspectorDrawer`'s VNC + Screen Sharing tabs, panel dispatch and `?inspector=` fallback | host connection + common-room status | removed from the strip; a media tab named in the URL degrades to Details, and is honoured the moment the wire can serve it |

Two media reads deliberately stay on the bare predicate. `ParticipantList`'s camera column sits
*inside* the rendered roster, which only exists once presence has resolved — while the room joins
the panel shows "Connecting…", so there is no strip to reflow. And `SessionRuntime`'s `carriesMedia`
is session-scoped and predates this node; a session connection exists only once the session is
attached, which is not a common-room join.

Three props became **required** rather than defaulted — `ParticipantList.connection`,
`RpcPlaygroundScreen.presenceAvailable`, `SessionDrawer.crossHostSessionsVisible` — following
`InspectorTabs.mediaAvailable` from the media milestone. A default would mean "unknown ⇒ show it",
which is the silence this node exists to remove: a list that has lost rows looks exactly like a list
that never had them. Six existing specs state the answer as a result; no assertion was changed.

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
