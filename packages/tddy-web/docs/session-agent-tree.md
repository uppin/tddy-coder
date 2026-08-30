# The Agents tab's tree

How `tddy-web` renders one session's main agent, the roster agents attached to it, and the subagent
sessions it spawned, as a single hierarchy.

Feature: [session-agent-roster.md § The Agents tab](../../../docs/ft/daemon/session-agent-roster.md),
[agent-session-status.md](../../../docs/ft/daemon/agent-session-status.md).
Sibling doc: [session-agent-conversation.md](session-agent-conversation.md), which covers talking to
an attached agent from a tab.

## Two feeds, one tree

The tab reads nothing new. It correlates two feeds the app already holds:

| Feed | Gives | Held by |
|---|---|---|
| `StreamSessionAgents` | The roster of one session — whole snapshots, never diffs | `useSessionAgentRoster`, one instance per rendered session node |
| `ListSessions` | Every session the browser can see, each with `orchestrator_session_id` | The drawer, threaded down as the `sessions` prop |

A **managed** roster agent reports `SessionAgentEntry.status`; a **non-managed** subagent session
reports `SessionEntry.agent_status`, which the daemon infers by tailing that session's own
conversation. Both are the same enum, deliberately, so one badge serves both.

## Modules

| Module | Responsibility |
|---|---|
| `agentTree.ts` | `subagentSessionNodes(sessions, rootSessionId)` — the pure fold. No React, no RPC. |
| `agentStatusDisplay.ts` | `agentStatusName` / `agentStatusToken` / `statusIsWorking` / `lastActivityText`. The vocabulary both row kinds draw from. |
| `sessionRosterHalf.ts` | `rosterHalfOf(session)` — which session, on which daemon, holds a roster. |
| `SessionAgentTree.tsx` | The rows: root, `ChildRows`, `RosterAgentRow`, `SubagentSessionRow`, plus the shared `AgentStatusBadge` and `LastActivityLine`. |
| `SessionAgentRosterPane.tsx` | Owns the root's roster stream, the picker, the attach and the detach confirmation; renders the tree. |

`SessionInspectorDrawer` passes the whole `SessionEntry` rather than a derived address, and the pane
derives its own roster half. A pane handed both the entry and a separately-derived address could be
given a mismatched pair.

## The fold

```ts
const childrenOf = (parentId: string, onBranch: ReadonlySet<string>) =>
  sessions
    .filter((s) => s.orchestratorSessionId === parentId && !onBranch.has(s.sessionId))
    .map((s) => ({ session: s, children: childrenOf(s.sessionId, new Set([...onBranch, s.sessionId])) }));

return childrenOf(rootSessionId, new Set([rootSessionId]));
```

Seeding `onBranch` with the root is what makes three rules one. The root is never its own subagent; a
session naming *itself* as orchestrator is dropped; and a cycle ends the branch rather than
descending twice. `ListSessions` is assembled from several hosts' answers, so a cycle is a shape to
survive rather than one to assume away.

A session whose orchestrator is not reachable from the root does not appear at all — promoting an
orphan would claim this agent spawned it. Siblings keep the input list's order, so two folds over one
list agree.

## One subscription per expanded node

`useSessionAgentRoster` is per-session and hooks cannot be called in a loop, so **each session node
is a component that subscribes for itself**, gated on its own `expanded` state through the hook's
existing `enabled` flag.

That gate is a cost decision. The tab is open for the life of the inspector; subscribing every
descendant on mount would hold one daemon stream per subagent for that whole time, for rows nobody is
looking at. It also makes the property testable without a flag: an unmounted subscription *is* an
unopened stream, and a spec asserts the exact list of session ids `StreamSessionAgents` was asked
about.

A subagent's own status needs no stream — it rides `ListSessions` — so a collapsed row still says
what it is doing.

`rosterHalfOf` is applied **per node**. A subagent can be split independently of its parent, and
reading the agent half would return an empty list beside the real one.

## Detach, from anywhere in the tree

A row does not detach itself. It raises a `RosterDetachRequest` carrying the entry, **the roster it
belongs to**, and the half holding it. The pane owns the one confirmation dialog.

Both extra fields are load-bearing: "the last agent of a remote daemon" is a fact about *that*
roster, not the root's, and the call has to be addressed to the daemon that holds it. A subagent row
raises nothing — there is no roster entry behind a session.

## Test ids

Roster rows keep `agent-roster-row-<qualified agent id>` and every sub-id they had, so the roster
specs that predate the tree are untouched. New ids are `agent-tree`, `agent-tree-root*` and
`agent-tree-session-<session id>*`. Every row carries `data-agent-kind` (`main` / `roster` /
`session`) and `data-depth`, so nesting is a fact in the DOM rather than a margin.

## Testing

| Aspect | Level | Why |
|---|---|---|
| The fold — cycles, self-reference, orphans, sibling order | `bun test` (`agentTree.test.ts`) | Pure and branchy. A cycle driven through a rendered tree either hangs the runner or proves nothing the fold does not already state. |
| Status vocabulary and relative age | `bun test` (`agentStatusDisplay.test.ts`) | Pure string functions shared by two row kinds — which is the reason to pin them centrally. |
| Nesting, per-node streams, badges, actions, the error row | Cypress (`SessionAgentSubagentTree.cy.tsx`) | "Landed under the right parent" and "opened no stream" are facts about a mounted hierarchy and a live fake. |
| Switch focuses the session | Cypress (`SessionAgentTreeAcceptance.cy.tsx`) | Spans `SessionsDrawerScreen` → `SessionMainPane` → Inspector. |

Containment is asserted by scoping into a parent's *children* list. Two independent existence checks
would pass against a flat list, which is the regression these specs exist to catch.

`RosterScenario` on the roster fake carries `rostersBySession` and `rosterFailuresBySession`, so one
backend can give each session its own roster and fail exactly one node's read — a fake that could
only answer or fail uniformly could not tell an agent nested under the right parent from one nested
under the wrong one.

## Known limitations

- **The tree sees only the sessions the browser lists.** A subagent on a host the browser is not
  aggregating is silently absent, and nothing on screen says the tree may be partial. A
  `subagent_count` on the parent would let a node say so; that is a proto and daemon change.
- **A subagent's roster has no loading state.** `error` is surfaced; `hasSnapshot` is not, so a node
  expanded against a slow host renders an empty child list for the moment before its first frame.
- **A cursor-cli subagent never appears.** Its `orchestrator_session_id` does not reach `SessionEntry`
  (`session_list_enrichment.rs`), so the fold cannot see it. A daemon fix, tracked in
  `docs/dev/TODO.md`.
