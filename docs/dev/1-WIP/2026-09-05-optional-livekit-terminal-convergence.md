# Changeset: optional-livekit-terminal-convergence

**Stack:** `optional-livekit` — node 5 of 7 (parent: `capability-gating`, base
`feature/optional-livekit/capability-gating`)
PR: [#441](https://github.com/uppin/tddy-coder/pull/441)
PRD: [`2026-09-05-optional-livekit-terminal-convergence-prd.md`](2026-09-05-optional-livekit-terminal-convergence-prd.md)
Discovery: [`2026-09-05-optional-livekit-terminal-convergence-initial-discovery.md`](2026-09-05-optional-livekit-terminal-convergence-initial-discovery.md)

## State A

Inherited from nodes 1–4: connections, directory, one session status, capability gating. Two terminal
components remain, and node 3 chose between them on capabilities rather than merging them.

- `GhosttyTerminalLiveKit.tsx` — 736 lines. Imports `livekit-client` (`Room`, `RoomEvent`,
  `DisconnectReason`), builds a `TerminalService` client via `useLiveKitTransportFactory`, owns
  connect/reconnect and token refresh (`TokenResult`, `getToken`), and shows the LiveKit status strip
  (`shouldShowVisibleLiveKitStatusStrip`, `isCancelledLiveKitConnectionError`). **No scrollback.**
- `GhosttyTerminalGrpc.tsx` — 631 lines. Takes an already-abstract `GrpcStream`
  (`send`/`onMessage`/`close`) and a `HistoryFetcher`; owns forward history paging
  (`TerminalHistoryForwardLoader`, `TerminalStreamOffset`, `PAGE_SCROLLBACK = 50000`) and the
  reconnect byte-buffering fix for the 220-column garbling.
- `SessionLiveKitTerminal.tsx` — 95 lines, the LiveKit variant's session-level wrapper.
- Both render the same chrome: `GhosttyTerminal`, `ConnectionTerminalChrome`,
  `TerminalConnectionStatusBar`, `ShortcutDrawer`, `MobileTerminalKeyboard`, `TerminalFileDropZone`,
  `TerminalUploadButton`.

## State B

- One `GhosttyTerminalSession`, taking a `TerminalStream` (+ optional `HistoryFetcher`) supplied by
  the session's `SessionConnection`. It imports no `livekit-client`, constructs no `Room`, mints no
  token.
- `TerminalStream` / `TerminalFrame` are `GrpcStream` / `GrpcFrame` promoted out of the gRPC
  component and renamed; the `SessionConnection` opens one.
- **Scrollback history works on a LiveKit session** — the history path applies wherever the
  connection offers a `HistoryFetcher`.
- `GhosttyTerminalLiveKit.tsx`, `GhosttyTerminalGrpc.tsx` and `SessionLiveKitTerminal.tsx` are
  deleted; every call site renders the one component.

## Responsibility

- `TerminalStream` / `TerminalFrame` / `HistoryFetcher` as the terminal's input contract, and the
  `SessionConnection` methods that produce them.
- `GhosttyTerminalSession` — the single component, carrying every behaviour both predecessors had.
- Deleting the two old components and `SessionLiveKitTerminal.tsx`, and migrating their call sites
  (`SessionRuntime.tsx`, `SessionMainPane.tsx`, the stories, the Cypress drivers).
- Extending history to LiveKit-carried sessions.
- Tests: the union of both components' existing coverage against the one component, plus new coverage
  for history over a LiveKit-carried connection.

## Boundaries

- Does **not** change `GhosttyTerminal` itself — the xterm-level widget is untouched.
- Does **not** change the terminal protocol, `terminal.TerminalService`, the daemon's PTY bridge
  (`cli_session_manager.rs`), or any proto.
- Does **not** define `SessionConnection`, `openSession`, capabilities or the connected status —
  node 3's. This PR **adds** the terminal-stream methods to `SessionConnection`, which are its own.
- Does **not** change capability gating decisions or add a new gated surface — node 4's.
- Does **not** touch the host directory or the connection provider registry.
- Does **not** add IPC or register a desktop provider — nodes 6 and 7. A terminal over IPC works
  through this same `TerminalStream` with no further change here.
- Adds **no npm dependency**.

## Dependencies

What the parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `capability-gating` (#440) | `useHasCapability`, media/presence surfaces gated, honest degradation in the sessions drawer — and, through it, nodes 1–3's connection model, directory and single session status | the one terminal reads its stream from the `SessionConnection` node 3 defined, and renders under the gating node 4 established | change `useHasCapability`, re-gate a surface, alter `openSession`/routing/caching, or reinstate a second connected status |

**Sequencing:** the terminal-stream methods this PR adds to `SessionConnection` extend an interface
node 3 owns. Adding a member is this PR's to do; changing an existing one is not — if a node-3
signature is wrong here, that is a plan change to raise, not to patch from this branch.

As with node 3 extending node 1's interface, adding `openTerminal` made one of node 3's **test
fixtures** fail to typecheck. This PR adds a one-line throwing `openTerminal` to
`cypress/component/SessionConnectionAcceptance.cy.tsx`. No production file of node 3's is touched.

## Draft PR contract

Lands first:

1. `TerminalStream`, `TerminalFrame`, `HistoryFetcher` in `src/rpc/connections/terminal.ts`, and the
   `SessionConnection` methods that open them, with final signatures.
2. `GhosttyTerminalSession`'s props.
3. Failing unit tests: forward history paging against a `TerminalStream` double; reconnect byte
   buffering (the 220-column regression guard); offset alignment on the initial replay frame.
4. Failing Cypress component tests: **scrollback on a LiveKit-carried session** (the behaviour that
   does not exist today), plus the existing status-strip, resize, zoom, drop/upload, mobile-keyboard
   and shortcut-drawer behaviours driven through the one component.

Deleting the two old components and migrating their call sites lands in the same PR under `/green`.
**Not a merge candidate on the contract alone.**

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests — `cypress/component/TerminalConvergenceAcceptance.cy.tsx`
- [x] Run acceptance tests (verify they fail) — 4/4 on `feedSupportsHistory`
- [x] USER REVIEW — acceptance tests — waived 2026-09-05 (run wave 2 straight through)
- [x] TDD Red — write failing unit/integration tests — `src/rpc/connections/terminal.test.ts`
- [ ] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap`

## Verification

### Baseline after rebasing onto the parent's contract commit

`bun run --filter tddy-web test:unit` — 971 pass, 17 fail, all inherited red from #437 (6), #439 (5)
and #440 (6). None is this node's to fix; this node adds 3 more.

The full Cypress component sweep ran once at node 1 (207 specs, 1213/1214; the one failure
pre-existing in `SelectedHostUrlStateAcceptance.cy.tsx`). One full sweep runs at the Step 8
completion gate.

### Red status at the contract commit

| Suite | Result |
|---|---|
| `src/rpc/connections/terminal.test.ts` | **3 tests, 3 failing** |
| `cypress/component/TerminalConvergenceAcceptance.cy.tsx` | **4 tests, 4 failing** |

Every failure is on this node's own `TODO(terminal-convergence)` body.

The acceptance spec's third test — *"offers scrollback on a LiveKit-carried session"* — is the one
worth watching during `/green`: it is the behaviour this node **adds**, not one it preserves. The
other three would pass against either predecessor component.

### Commands

```bash
./dev bun run --filter tddy-web test:unit
./dev bun run --filter tddy-web cypress:component
./dev bun run --filter tddy-web cypress:e2e     # ghostty-* suite — NOT in the CI gate; run locally
```

Installs via `bun run local-registry-install` — the public npm registry is unavailable.

## Successor PRs

None — leaf node. `feature/optional-livekit/desktop-ipc-host` is its sibling off the same base.
