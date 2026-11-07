# 2026-11-07 — Participant-list test ids are defined twice

**Category:** Duplication (test support)
**Source:** `#web-split` 6/17 (PR #843), `/green`

`ParticipantList` moved to `tddy-livekit-web`, so its test ids are built in
`packages/tddy-livekit-web/cypress/support/testIds.ts`. `packages/tddy-web/cypress/support/testIds.ts`
still builds the same ids, because `tddy-web` specs (`SessionsDrawerAcceptance`,
`LiveKitRoomsPanelAcceptance`, …) drive the roster through `tddy-web` screens. Both copies call
`safeTestIdPart`, so they agree today; a change to one id format would silently miss in the other's specs.

**Why deferred:** the `tddy-web` specs that select the roster stay until the nodes that move their
screens (`hosts`, `session`); deleting the `tddy-web` copy now breaks them, and exporting Cypress
support from a package's runtime entry is not something any `#web-split` package does.

**What closing it takes:** when the last `tddy-web` spec selecting the roster moves, delete the
`tddy-web` copy; or, earlier, export the ids from `tddy-livekit-web/testkit` and import them in
`tddy-web`'s support. Natural owner: the `#web-split` sweep node.
