# Changeset: optional-livekit-capability-gating

**Stack:** `optional-livekit` — node 4 of 7 (parents: `host-directory`, `session-connection`;
PR base `feature/optional-livekit/session-connection`, with `host-directory` merged in through the
local integration ref `stack-int/capability-gating`)
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
| `session-connection` (#439) | `SessionConnection` with `capabilities`, one connected `SessionAttachmentState`, capability-driven handshake overlay and terminal selection | session-scoped media gating reads the session connection's capabilities | change `openSession`, the hint type, routing, the client cache, or terminal selection |

**Sequencing:** both parents have pushed their contract commits. This node's PR base is
`session-connection`; `host-directory`'s commits arrive through the local `stack-int/capability-gating`
integration ref, and this PR is only offered for merge once **both** parents have merged.

One consequence to hold on to during `/green`: **`useHostPresence` is not on this PR's head.** It is
node 2's, and it reaches this branch only through the integration ref. So the *contract* committed
here is scoped to what stands alone — `useHasCapability` and the gating rule its tests pin. Wiring the
presence-derived surfaces (`ParticipantList`, `LiveKitRoomsPanel`, `LiveKitAppPage`, the playground
roster, the sessions drawer's cross-host reconciliation) needs `useHostPresence`, so `/green` must run
in a worktree that has the integration ref, or wait for `host-directory` to merge. Wiring the *media*
surfaces has no such dependency.

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
