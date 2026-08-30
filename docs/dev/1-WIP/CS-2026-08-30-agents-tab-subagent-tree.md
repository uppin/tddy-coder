# Changeset: Subagents as children of the main agent in the Agents tab

**Created:** 2026-08-30
**Status:** In progress
**PRD:** docs/ft/web/1-WIP/PRD-2026-08-30-agents-tab-subagent-tree.md
**Stacked on:** #422 (`feature/agents-revamp/remove-peer-spawn-add-agent`)

## Affected Packages

- [x] `tddy-web` — the whole change.
- [ ] No `.proto` edit, no Rust. Both upstream halves (#410, #419) are already on master.

## State A (Current)

- `SessionAgentRosterPane` renders a flat `<ul>` over `useSessionAgentRoster(sessionId)`. Each row
  shows label, qualified id, model, host, clone state, status badge, last-activity line, `replaces`
  and a Detach button. It knows nothing about subagent *sessions*.
- `SessionInspectorDrawer` receives `session: SessionEntry | null` and **not** the session list. It
  owns `rosterHalfOf`, the private rule that a split session keeps its roster on its codebase half.
- `SessionMainPane` derives `peers` with `sessionPeers(sessions, selectedSession.sessionId)` and
  renders `SessionAgentsSection` — one row per peer, labelled with `SessionEntry.status`
  ("active"/"idle"/"needs-input"), which is the session's *lifecycle* string and says nothing about
  what the agent inside it is doing. Each row has a Switch button wired to `onSwitchPeer`.
- `src/gen/connection_pb.ts` was **stale** when this work began: its `SessionEntry` ended at
  `codebase_session_id` (30), so `agent_status` (31) and `last_activity` (32) — shipped by #419 —
  were unreachable from the web. It was regenerated here, and then **master regenerated it too**
  (#421/#422 landed mid-flight). After the rebase onto master the file is byte-identical to master's,
  so it is not in this diff. Recorded because the gap was real and the next reader will wonder.
- `agentStatusName` / `agentStatusToken` / `statusIsWorking` / `lastActivityText` are private to
  `SessionAgentRosterPane.tsx`, so nothing but a roster row can render that badge.

## State B (Target)

- The Agents tab renders **one tree**: the session's main agent at the root, its roster agents and
  its subagent sessions beneath it, and a subagent session's own roster agents and subagents beneath
  *that*, to arbitrary depth.
- One badge renders both kinds, from one vocabulary module. A managed row reads
  `SessionAgentEntry.status`; a non-managed row reads the inferred `SessionEntry.agent_status`.
- A roster row keeps Detach; a subagent session row offers Switch instead.
- A subagent session opens a roster stream only while expanded, addressed to its own codebase half.
- `SessionAgentsSection` and `sessionPeers` are gone — the tree is the one place peers are listed.

## Delta

### New

- **AC24, added during PR wrap.** Production-readiness review found that an expanded subagent whose
  roster read failed rendered as a subagent with no agents — a silent fallback of exactly the kind
  `CLAUDE.md` forbids, and the conflation the pane already refuses at its own root. Fixed with a
  per-node error line (`agent-tree-session-<id>-roster-error`), two Cypress cases, and a
  `rosterFailuresBySession` scenario on the roster fake so one node's host can fail while the rest of
  the tree reads fine. The developer approved the scope addition.
- `src/components/sessions/agentTree.ts` — the pure fold. `subagentSessionNodes(sessions, rootId)`
  returns a recursive `SubagentSessionNode[]`, excluding the root, dropping self-references and
  orphans, and terminating on a cycle. No React, no RPC.
- `src/components/sessions/agentStatusDisplay.ts` — `agentStatusName`, `agentStatusToken`,
  `statusIsWorking` and `lastActivityText`, **moved** out of `SessionAgentRosterPane.tsx` so a
  session row and a roster row cannot word the same status differently.
- `src/components/sessions/sessionRosterHalf.ts` — `rosterHalfOf`, **moved** out of
  `SessionInspectorDrawer.tsx`, because every session node in the tree now needs the same rule, not
  just the root.
- `src/components/sessions/SessionAgentTree.tsx` — the tree. Three pieces: the root row, a roster
  agent row, and a recursive subagent session row that owns its own `useSessionAgentRoster` while
  expanded.
- `cypress/component/SessionAgentSubagentTree.cy.tsx` — the pane-level acceptance suite.
- `cypress/component/SessionAgentTreeAcceptance.cy.tsx` — the screen-level suite through
  `SessionsDrawerScreen`.
- `src/components/sessions/agentTree.test.ts`, `src/components/sessions/agentStatusDisplay.test.ts`.

### Modified

- `src/components/sessions/SessionAgentRosterPane.tsx` — the flat `<ul>` becomes `<SessionAgentTree>`.
  New props `sessions` and `onSwitchSubagent`. The picker, the attach, the detach confirmation and
  the four exclusive states are untouched.
- `src/components/sessions/SessionInspectorDrawer.tsx` — `sessions` and `onSwitchPeer` threaded to
  the roster pane; `rosterHalfOf` imported rather than declared.
- `src/components/sessions/SessionMainPane.tsx` — the `SessionAgentsSection` render and the `peers`
  memo are dropped; `sessions` and `onSwitchPeer` are passed to `SessionInspectorDrawer` instead.
- `cypress/support/rpc/sessionAgentRosterBackend.ts` — `RosterScenario.rostersBySession`, so one
  backend can answer `StreamSessionAgents` differently per `session_id`. The existing `initial` stays
  the answer for any session the map does not name, so no current spec changes.
- `cypress/support/pages/sessionAgentRosterPage.ts` — tree helpers (root row, subagent row, expand,
  switch, depth, kind).
- `cypress/support/testIds.ts` — the `agentTree*` ids; the `sessionAgents*` ids are removed with the
  section they named.

### Repointed (test-side, in the red phase)

The pane's props become `session` / `sessions` / `sessionToken` / `daemonConnected` /
`onSwitchSubagent`: `sessionId` and `daemonInstanceId` are **derived** from `session` by
`rosterHalfOf` inside the pane, rather than derived by the drawer and passed in. The tree needs the
whole entry anyway — the root row is the session's own agent, and its children are matched by
`orchestrator_session_id` — and a pane that took both the entry and a separately-derived address
could be handed a mismatched pair.

The three existing roster specs' mount helpers were repointed to that contract as part of writing
the failing tests, so the API is pinned by tests rather than discovered during implementation:
`SessionAgentRosterPane.cy.tsx`, `SessionAgentRosterStatus.cy.tsx`,
`SessionAgentRosterSplitSession.cy.tsx`. No case's assertions changed. The split-session spec gets
*stronger*: it used to be handed the codebase half, and now states the split and leaves the pane to
resolve it.

### Removed

- `src/components/sessions/SessionAgentsSection.tsx` and `cypress/component/SessionAgentsSection.cy.tsx`
  (4 cases). The population it lists is now a branch of the tree, with a real status instead of the
  session's lifecycle string.
- `cypress/component/SessionMainPanePeerSwitch.cy.tsx` (3 cases) and
  `cypress/support/pages/sessionAgentsPage.ts`. Their subject — "the main pane lists peers and
  switches to one" — is denied by AC23; the switch behaviour is re-covered in
  `SessionAgentTreeAcceptance.cy.tsx`, where it now lives.
- `src/utils/sessionPeers.ts` and `src/utils/sessionPeers.test.ts` (superseded by `agentTree.ts`,
  which answers the same question recursively and is the only caller left).

## Milestones

### Milestone 0: Plan and pin the contract
- [x] Create PRD documentation
- [x] Create changeset
- [x] Write the failing acceptance tests and unit tests, verified failing for missing implementation
- [x] Confirm the existing roster / inspector suites still pass

### Milestone 1: The pure fold and the shared vocabulary
- [x] `agentTree.ts` + tests
- [x] `agentStatusDisplay.ts` + tests, `SessionAgentRosterPane` importing them
- [x] `sessionRosterHalf.ts`, the roster pane deriving its own address with it

### Milestone 2: The tree
- [x] `SessionAgentTree.tsx` — root row, roster row, recursive subagent row
- [x] Lazy roster subscription per expanded session node
- [x] `SessionAgentRosterPane` renders it; `sessions` / `onSwitchPeer` threaded from `SessionMainPane`

### Milestone 3: Retire the old section
- [x] Delete `SessionAgentsSection`, `sessionPeers` and their specs
- [x] Screen-level suite green

## Testing Strategy

### Test Level Decisions

| Aspect | Level | Rationale |
|---|---|---|
| List → tree fold (cycles, self-reference, orphans, order) | Unit (`bun test`) | Pure and branchy. A cycle driven through a rendered tree either hangs the runner or proves nothing the fold does not already state. |
| Status vocabulary + relative age | Unit (`bun test`) | Pure string functions, now shared by two row kinds — which is exactly the reason to pin them once, centrally. |
| Nesting, per-node streams, badges, actions | Cypress component (pane) | "Landed under the right parent" and "opened no stream" are facts about a mounted hierarchy and a live fake. Neither survives a unit test of the fold. |
| Switch focuses the session; the old section is gone | Cypress component (screen) | Spans `SessionsDrawerScreen` → `SessionMainPane` → Inspector. A narrower mount lets the three disagree in silence. |

### Why the pane-level suite mounts `SessionAgentRosterPane` and not the tree

`SessionAgentTree` is where the nesting is decided, but the pane is where the roster stream, the
`sessions` prop and the detach flow meet. Mounting the tree alone would need the spec to hand it a
roster it made up, which is the fixture proving itself. Mounting the pane keeps the daemon fake as
the only source of a roster row.

## Implementation notes

- **A session node owns its own subscription.** `useSessionAgentRoster` is per-session by
  construction, and hooks cannot be called in a loop, so each session row is a component that
  subscribes for itself. That is also what makes AC20 expressible: an unmounted subscription is an
  unopened stream, not a flag.
- **Collapsed by default is a cost decision, not a style one.** The Agents tab is open for the life
  of the inspector; subscribing every descendant on mount would hold one daemon stream per subagent
  for that whole time, for rows the operator is not looking at. A subagent's *own* status needs no
  stream — it rides `ListSessions` — so a collapsed row still shows what it is doing.
- **`rosterHalfOf` applies per node, not once.** A subagent session can be split independently of its
  parent, and reading its roster on the agent half would return an empty list beside the real one.
- **A subagent session the daemon does not tail reports `UNSPECIFIED`.** `ensure_tailing` gates on
  `claude-cli` / `cursor-cli`, so a `tool` or `workspace` subagent is a node with an honest
  "unknown", never an invented "idle".

## Verified — red phase

Run from this worktree on 2026-08-30.

| Suite | Result |
|---|---|
| `bun test src` (whole `packages/tddy-web/src`) | **914 passing, 2 failing** — the two new unit files, each failing on the module it defines (`./agentTree`, `./agentStatusDisplay`). Nothing else moved, so the `connection_pb.ts` regeneration broke no existing test. |
| `SessionAgentSubagentTree.cy.tsx` (new, 22) | **21 failing, 1 passing.** Every failure is a missing tree id (`agent-tree-root`, `agent-tree-root-children`, `agent-tree-session-*`). The one passing case asserts the empty state the current pane already satisfies — kept because the requirement is real (the pane's four states must survive the tree), not because it is red. |
| `SessionAgentTreeAcceptance.cy.tsx` (new, 4) | **4 failing.** Three on the missing tree; the fourth on the `Session agents` section still being rendered, which is precisely AC23. |
| `SessionAgentRosterPane.cy.tsx` (repointed, 15) | **14 passing, 1 failing** — "detaches a local agent without asking". The pane still reads the removed `daemonInstanceId` prop, so `detachDeletesACheckout` no longer recognises a local agent and asks for a confirmation it should not. Red for exactly the derivation this change moves into the pane. |
| `SessionAgentRosterStatus.cy.tsx` (repointed, 10) | 10 passing. |
| `SessionAgentRosterSplitSession.cy.tsx` (repointed, 4) | 4 passing. |
| Regression: `SessionAgentsSection`, `SessionMainPanePeerSwitch`, `SessionInspectorAcceptance`, `SessionAgentAttachTabAcceptance`, `SessionAgentConversationPane` | **44 passing, 0 failing.** The two peer specs still pass and are deleted in Milestone 3. |

`cargo fmt` / `clippy` / `test` are not run: no `.rs` or `.proto` file is touched.

## Verified — green phase

Run from this worktree on 2026-08-30.

| Suite | Result |
|---|---|
| `bun test src` (whole `packages/tddy-web/src`) | **980 passing, 0 failing** (1296 assertions, 111 files), re-run after the rebase onto current master. |
| `SessionAgentSubagentTree.cy.tsx` (22) | **22 passing.** |
| `SessionAgentTreeAcceptance.cy.tsx` (4) | **4 passing.** |
| `SessionAgentRosterPane.cy.tsx` (15) / `SessionAgentRosterStatus.cy.tsx` (10) / `SessionAgentRosterSplitSession.cy.tsx` (4) | **29 passing.** The one red case — "detaches a local agent without asking" — is green now that the pane derives its own roster half. |
| Regression: `SessionInspectorAcceptance` (14), `SessionAgentAttachTabAcceptance` (12), `SessionAgentConversationPane` (11), `SessionInspectorSplitRoster` (3), `SessionInspectorUrlStateAcceptance` (9), `SessionInactiveInspectorOverlay` (6) | **55 passing, 0 failing.** |
| Targeted Cypress after the rebase — the two tree specs plus roster, inspector, split-roster, files-tab, attach-tab and conversation-pane | **101 passing, 0 failing.** |
| Whole Cypress component suite (199 specs) | **Not run against the final rebased tree.** A full sweep passed at 1156/0 earlier in the wrap, before the AC24 fix and before the rebase onto master. The 10 suites above are the ones this change can reach; the shared-component extraction (`AgentStatusBadge` / `LastActivityLine`) is exercised by all of them. Worth a CI sweep. |
| `bun run build` | Clean, exit 0. |
| `tsc --noEmit` | Not a gate here, and unchanged in kind: no error names any file this change adds or edits, beyond the `bun:test` module resolution 110 other unit files already report. |

`cargo fmt` / `clippy` / `test` are not run: no `.rs` or `.proto` file is touched.

## Technical Debt

- **A subagent's roster is not shown as *loading*.** `SubagentSessionRow` now surfaces `error`
  (AC24), but still discards `hasSnapshot`, so a node expanded against a slow host renders an empty
  child list for the moment before its first frame. Transient and not a false claim in the way the
  error case was — an empty list that becomes populated is not an empty list that is wrong — but a
  per-node "Loading agents…" would match what the root does.
- **Expanding a deep tree opens one stream per expanded node, and nothing shares them.** Two nodes
  addressing the same roster half (a subagent split onto the same codebase host as its parent) open
  two `StreamSessionAgents` calls. Collapsed nodes cost nothing, which is what bounds this in
  practice, but a subscription cache keyed by `RosterHalf` would bound it properly.
- **Collapsing a node does not clear the roster it last read.** `useSessionAgentRoster` returns early
  when `enabled` goes false rather than resetting, so a re-expanded node paints the previous roster
  for one frame before its new snapshot lands. Invisible today because a collapsed node renders no
  children at all; it becomes visible the moment a collapsed node shows a child count.
- **Two product docs still describe the deleted section** and are stale as of this change:
  `docs/ft/web/session-drawer.md:657-660` (§ Session Agents, naming `SessionAgentsSection.tsx` and
  `sessionPeers.ts`) and `docs/ft/daemon/session-agent-roster.md:76` (a table row contrasting the
  roster with `SessionAgentsSection.tsx`). Deliberately not edited here — `docs/ft/` is changed
  through the wrap step, not from a feature diff. **Both must be updated by `/wrap-context-docs`
  before this merges.**
