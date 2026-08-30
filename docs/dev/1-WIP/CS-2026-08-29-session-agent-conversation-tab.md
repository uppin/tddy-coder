# Changeset: Talk to a session's attached agent from a conversation tab

**Created:** 2026-08-29
**Status:** Complete
**PRD:** docs/ft/web/1-WIP/PRD-2026-08-29-session-agent-conversation-tab.md

## Affected Packages

- [x] `tddy-web` — the whole change. No proto, no Rust.

## State A (Current)

- `SessionMainPane`'s header **Add agent** button (`session-agents-add-btn`) navigates to
  `#/sessions/:id/add-agent`. `peerCreateInitialValues` derives a pre-fill from the selected session,
  `CreateSessionPane` renders in `peerMode`, and submitting calls **`StartSession`** with
  `stackParent` — spawning a peer *session* on the same worktree.
- `appRoutes.ts` carries `ADD_AGENT_SEGMENT`, `sessionsDrawerAddAgentPath`, `rawAddAgentSegment` and
  `parseSessionsDrawerAddAgentSessionId`; `isSessionsDrawerPath` and `parseSessionsDrawerSessionId`
  both accept the add-agent path.
- `CreateSessionPane` has a `peerMode` prop gating ~14 code paths (host fan-out, effective
  project/daemon, offered agents, branch/worktree blanking on submit, and several render gates).
- Roster attach exists only in `SessionAgentRosterPane` (Inspector → Agents tab), which can attach and
  detach but cannot talk to an agent.
- `SessionTerminalTabs` renders three tab kinds: the fixed Agent tab, bash tabs, and spawned
  child-conversation tabs. Its parent `SessionRuntime` owns `activeTerminalId` / `activeChildSessionId`.
- Nothing in `src/` calls `OpenAgentConversation` / `PromptAgentConversation` /
  `CancelAgentConversation`; they exist only as generated types in `src/gen/connection_pb.ts`.

## State B (Target)

- The header button (`session-agent-attach-btn`) opens a fanned-out agent picker, attaches the picked
  agent to the **current** session via `AttachSessionAgent`, opens a conversation via
  `OpenAgentConversation`, and focuses a new **agent conversation tab**.
- That tab's body is an interactive transcript: the operator prompts, `PromptAgentConversation`
  streams the answer back in chunks, the final frame's `stop_reason` closes the turn.
- Closing the tab cancels the conversation and returns focus to the Agent tab.
- The peer-spawn flow is gone from `SessionMainPane`, from `appRoutes`, and from `CreateSessionPane`.

## Delta

### New

- `src/components/sessions/agentConversationTranscript.ts` — pure projection of
  `AgentConversationChunk` frames into turns (`appendOperatorTurn`, `appendAnswerChunk`).
- `src/components/sessions/agentConversationTabs.ts` — pure tab-list algebra
  (`AgentConversation`, `agentConversationLabel`, `withAgentConversation`, `conversationForAgent`).
- `src/components/sessions/useAgentConversation.ts` — opens the conversation on mount, sends prompts,
  folds the answer stream through the projection, cancels on close.
- `src/components/sessions/SessionAgentConversationPane.tsx` — the tab body (transcript + composer).
- `src/components/sessions/AgentPicker.tsx` — the fanned-out picker, **extracted** from
  `SessionAgentRosterPane`'s inline picker so there is exactly one. Takes an explicit `testIdPrefix`
  (`agent-roster-picker` for the roster pane, `session-agent-picker` for the header) so the two mounts
  never collide on a test id.
- Fixture `cypress/support/rpc/agentConversationBackend.ts` — `anAgentConversationFake` /
  `anAgentConversationBackend`, mirroring `sessionAgentRosterBackend.ts` (handlers + controls).
- Page object `cypress/support/pages/sessionAgentConversationPage.ts`.

### Modified

- `src/components/sessions/SessionMainPane.tsx` — header button rewired; picker + attach + open
  wired; agent conversations held per session and passed down through the runtime layer.
- `src/components/sessions/SessionTerminalTabs.tsx` — a fourth tab kind: `agentConversations`,
  `activeAgentConversationId`, `onSelectAgentConversation`, `onCloseAgentConversation`.
- `src/components/sessions/SessionRuntime.tsx` — renders the agent tab bodies and owns their focus,
  the way it already does for child tabs.
- `src/components/sessions/SessionAgentRosterPane.tsx` — picker JSX replaced by `<AgentPicker>`.
- `cypress/support/rpc/connectionServiceBackend.ts` — new `agentConversations` scenario; roster and
  conversation controls exposed on `ConnectionServiceBackend` (currently the roster fake's controls
  are built and discarded at `:394`).
- `cypress/support/testIds.ts`, `cypress/support/pages/sessionTerminalTabsPage.ts`.

### Removed

- `SessionMainPane`: `handleAddAgent` (peer nav), `peerCreateInitialValues`, `peerCreateCaptureRef`
  and its effect, `handlePeerCreated`, `handleCancelPeerCreate`, `isPeerCreating`, `activePeerMode`,
  and the `CreateSessionPane` peer branch.
- `appRoutes.ts`: `ADD_AGENT_SEGMENT`, `sessionsDrawerAddAgentPath`, `rawAddAgentSegment`,
  `parseSessionsDrawerAddAgentSessionId` (+ their 4 cases in `appRoutes.test.ts`).
- `CreateSessionPane`: the `peerMode` prop and every gate on it.
- `cypress/component/SessionsDrawerPeerSpawn.cy.tsx` (deleted — the flow it covers is gone).
- The `peerMode` mounts in `CreateSessionAgentHostFanOut.cy.tsx` and
  `CreateSessionCodebaseHostAcceptance.cy.tsx`. The fan-out mount becomes `mountFormForHost`, which
  points the form at a host through `initialValues.daemonInstanceId` and still proves the opening
  agent follows the session's host rather than the transport's. Four cases go with the prop, because
  what they asserted was peer mode's *withdrawals* and the standalone flow contradicts each of them:
  "offers only the agents of the host the peer will run on" and "stays silent about a host the peer
  will not run on failing to answer" are denied by AC1 and AC6 in the same spec, and
  `CreateSessionCodebaseHostAcceptance`'s "does not offer a codebase host when joining an existing
  worktree as a peer" is denied by the selector's own availability rule once `peerMode` is gone.
- `SessionMainPanePeerSwitch.cy.tsx`'s "renders the Add agent button" case, replaced by the new
  header coverage. Its peer-*switch* cases stay — `SessionAgentsSection` and `sessionPeers` are
  untouched, because peers still arrive from `spawn_conversation`.

## Milestones

### Milestone 0: Plan and pin the contract — **done**
- [x] Create PRD documentation
- [x] Create changeset
- [x] Write the failing acceptance tests (28 Cypress cases) and unit tests (15 `bun test` cases),
      verified failing for missing implementation rather than for a fixture bug
- [x] Confirm the 22 existing cases over the shared `connectionServiceBackend` still pass

### Milestone 1: Remove the peer-spawn flow — **done**
- [x] Strip the peer branch from `SessionMainPane`
- [x] Drop the add-agent route helpers and their unit tests
- [x] Drop `CreateSessionPane.peerMode` and rewrite the two specs that mount it
- [x] Delete `SessionsDrawerPeerSpawn.cy.tsx`

### Milestone 2: Attach from the header — **done**
- [x] Extract `AgentPicker` from `SessionAgentRosterPane` with an explicit `testIdPrefix`
- [x] Wire the header button: picker → `AttachSessionAgent` → the conversation tab, which opens the
      conversation itself (see Implementation notes)

### Milestone 3: The conversation tab — **done**
- [x] `agentConversationTranscript.ts` + `agentConversationTabs.ts`
- [x] `useAgentConversation` + `SessionAgentConversationPane`
- [x] The fourth tab kind in `SessionTerminalTabs` / `SessionRuntime`

## Testing Strategy

### Acceptance Tests — written and failing

- [x] `SessionAgentAttachTabAcceptance.cy.tsx` — 11 cases: the attach-then-open-tab flow, driven
      through `SessionsDrawerScreen` so the header, the RPCs and the tab strip are all real
      (AC1-AC4, AC10-AC12). All 11 fail on the missing `session-agent-attach-btn`, which is also
      what proves the fixture reaches the header before it stops.
- [x] `SessionAgentConversationPane.cy.tsx` — 9 cases: the tab body against a conversation backend
      (AC5-AC9). Fails at the import of the component that does not exist yet.
- [x] `SessionTerminalTabsAgentTabs.cy.tsx` — 8 cases: the tab strip's fourth tab kind in isolation.
      7 fail on the missing `sessions-agent-tab-*`; the eighth ("renders no agent tabs when no
      conversation is open") asserts an absence the current component already satisfies, so it is
      green from the start. Kept because the requirement is real, not because it is red.
- [x] `agentConversationTranscript.test.ts` (7) and `agentConversationTabs.test.ts` (8) — the pure
      modules, failing on the missing modules themselves.

### Test Level Decisions

| Aspect | Level | Rationale |
|---|---|---|
| Chunk → turn projection | Unit (`bun test`) | Pure and branchy: first chunk, continuation, terminal frame, empty answer, a second answer after a completed one. Cheap to pin exhaustively, expensive to drive through a stream. |
| Tab-list algebra (dedupe by agent) | Unit (`bun test`) | Pure. The "attaching twice must not open a second tab" rule is a list property, not a rendering one. |
| Picker → attach → tab | Cypress component (screen) | The property is that three collaborators agree: a header in `SessionMainPane`, RPCs on the wire, a tab strip inside `SessionRuntime`. Mounting `SessionMainPane` alone cannot show the tab. |
| Prompt → streamed answer | Cypress component (pane) | The stream is the subject; the in-memory backend can yield chunk-by-chunk, which no unit test of the pane could. |
| Peer-spawn removal | Cypress component | The observable claim is "clicking Add agent no longer opens the create pane" — a rendering fact. |
| Route helper removal | Unit (`bun test`) | Deleting the 4 cases in `appRoutes.test.ts` is the whole change. |

## Implementation notes

- **Mint the `conversation_id` with `src/lib/randomId.ts`, not `crypto.randomUUID`.** tddy-web is
  routinely served over plain http on a LAN address, which is not a secure origin, and
  `crypto.randomUUID` is `undefined` there. `OpenAgentConversation` accepts a caller-chosen id
  precisely so the caller can still name — and therefore cancel — what it opened
  (`packages/tddy-service/proto/connection.proto:479-481`), so the browser mints it rather than
  taking the daemon's.
- **`OpenAgentConversation` is issued by the tab's body, not by the header.** The header mints the
  `conversation_id` and opens the tab; `useAgentConversation` opens the conversation on mount and
  cancels it on unmount. One owner for the conversation's whole life is what keeps exactly one open
  per tab — the header opening it as well would open every conversation twice — and it is what makes
  a failed open surface in the tab that holds it rather than in the picker that is already closed.
- **Route the conversation RPCs the way the roster pane routes its own**: over the shared transport
  with the session's facilitating `daemon_instance_id` in the request. The daemon runs a local
  roster entry in-process and forwards a remote one to its owning daemon, so the web never has to
  know which host answered.

## Validation Results

Four review passes (change risk, test quality, production readiness, clean-code metrics) ran over the
whole diff. **Three defects were found and fixed**; the rest are recorded below or as debt.

### Fixed

1. **Enter bypassed the one-turn-at-a-time gate.** The Send button was `disabled={answering}` but the
   input's `onKeyDown` called `send()` unguarded, so a second prompt could be sent into an answer
   still arriving. That is not cosmetic: `appendAnswerChunk` extends *the open agent turn*, so the
   first stream's next chunk would open a turn the second stream then extends — two answers merged
   into one, with an operator turn stranded mid-answer, and whichever stream ended first clearing
   `answering`. The gate moved into `send()` so both ways in share it; the `disabled` prop now only
   reflects it. Covered by two new cases in `SessionAgentConversationPane.cy.tsx`.
2. **A cancel could be sent for a conversation that never opened.** The cleanup fired
   `CancelAgentConversation` unconditionally and without waiting for the open. On a fast
   open-then-close the cancel could land first (daemon answers NOT_FOUND) and the open land after,
   leaving a conversation — and the agent session `open_agent_conversation` spawns for it — running
   with nothing left to cancel it. The cleanup now awaits the open and cancels only what opened.
   Covered by "does not cancel a conversation the daemon refused to open".
3. **A double-click on Attach could leave a blank pane.** `attachAgent` read the conversation list
   from the closure captured *before* its `await`, so two attaches of the same agent both minted an
   id; the loser's id was then focused although the list had kept the winner's, leaving no tab
   selected and no pane rendered. `AgentPicker` now gates concurrent confirms.

Also corrected: `daemonInstanceId` was re-derived per render with `sessions.find(...) ?? ""`, and an
empty value is not "unknown" on the wire — it means "whichever daemon this request reached". It is
now stamped on the `AgentConversation` at attach time, when it is known for certain, which also
removes an O(runtimes x sessions) scan per render.

Test-quality fixes: the close-tab case asserted only a cancel *count* (the wrong conversation would
have passed); the withdrawal-warning fixture used `replaces` values that overlapped the builder's
default `tools` (a picker rendering the wrong field would have passed); the picker-offer case probed
for two options instead of stating the offer; a raw selector moved into the page object; and
`failNextPrompt` now throws instead of silently no-opping when its scenario is absent.

### Accepted, not fixed

- `SessionMainPane.tsx` (560 lines) and `SessionRuntime.tsx` (587) are over the 500-line guideline.
  Both were over before this change (539 / 515). See Technical Debt for the extraction each wants.
- `CreateSessionPane.tsx` is 1319 lines — **62 fewer** than before, since this change only deleted
  from it. Splitting it is its own PR: six Cypress specs read its test ids, and a structural move
  under that contract should be the only thing in a diff.

## Verified

All four gates run from this worktree on 2026-08-30:

| Suite | Result |
|---|---|
| `bun test` over `src/components/sessions` + `src/routing` | **341 passing, 0 failing** (35 files) |
| New Cypress: tab strip / pane / attach flow | **8 + 11 + 12 = 31 passing** |
| Regression: 10 suites over the roster, inspector, tabs, peers and create-session fan-out | **89 passing, 0 failing** |
| `bun run build` (vite) | **exit 0** (the >500 kB chunk warning is pre-existing) |
| `tsc --noEmit` | 465 errors repo-wide, **all pre-existing**; none in a file this change touched |

`tsc --noEmit` is not a CI gate here and master carries pre-existing type errors
(docs/dev/guides/ci.md); it was run for information and every error it reports sits outside this
change's hunks. `cargo fmt` / `clippy` / `test` are not run because no `.rs` or `.proto` file is
touched.

## Technical Debt

- The main agent's own sub-agent turns remain unobservable from the web. Making them replayable needs
  a daemon-side transcript sink plus an agent axis on `StreamAcpReplayRequest`, and
  `stream_acp_replay` still returns `UNIMPLEMENTED` for a peer forward
  (`connection_service.rs:13492`), which a remote roster agent would need. Tracked in
  `docs/dev/TODO.md` under Future Enhancements.
- `sessionPeers.ts` and `useChildSessions.ts` duplicate the same `orchestratorSessionId` predicate
  with two return shapes. Untouched here, and the duplication is unchanged — but the *justification*
  for two modules did not survive this change. "Peer agents I spawned from the header" and "child
  conversations a workflow spawned" used to be different populations; the header no longer spawns
  anything, so both now describe the same set, rendered twice on screen (the Session agents list and
  the child tabs). Collapsing to one `useChildSessions` returning `SessionEntry[]` costs two call
  sites and folding `sessionPeers.test.ts` into its tests.
- `SessionMainPane.tsx` (560) and `SessionRuntime.tsx` (587) are over the 500-line guideline, both
  from before this change. The cheapest extraction for the first is a `useSessionAgentConversations`
  hook holding the two per-session maps plus `attachAgent` / `focusConversation` / `closeConversation`
  (~85 lines out, no call sites move, no tests repoint, lands the file at ~478). For the second it is
  `useRuntimeFocusGuard` and `useSessionRuntimeClients` (~90 lines out, no shared state to widen).
  Deliberately not done here so the feature diff stays reviewable.
- The per-session conversation maps are never pruned when a session leaves the list. Growth is
  trivial, but a resumed session keeps its `sessionId`, so its tabs return pointing at conversations
  the daemon has long dropped, which then re-open under ids it has already seen.
- Conversation panes live inside the runtime layer, which is not rendered at all for a workflow view
  (PR-Stack, workflow chat) or while the create-session pane is open. Switching to such a session
  therefore unmounts every conversation pane and cancels its conversation, while the tabs survive in
  state — which contradicts the PRD's "switching sessions or tabs does not tear a conversation down".
  Hoisting the panes out of the runtime layer is the fix; it is a layout change, not a wiring one.
- The header's Add-agent button renders for any session with a client, including workflow, PR-Stack
  and dormant sessions that have no tab strip. The attach succeeds and nothing visible happens. Hide
  it where no tab strip exists, or say in the UI why the agent cannot be talked to there.
- The conversation body's inner test ids (`agent-conversation-*`) are not keyed by conversation, and
  every open conversation stays mounted. Two open tabs make `agent-conversation-input` ambiguous, so
  a spec that prompts with two tabs open would fail on a multi-element match. No current spec does.
