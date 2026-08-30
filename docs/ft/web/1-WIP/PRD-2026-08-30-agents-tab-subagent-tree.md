# PRD: Subagents as children of the main agent in the Agents tab

**Created:** 2026-08-30
**Product Area:** web
**Status:** Delivered

## Summary

The Inspector's **Agents** tab stops being a flat list of attached roster agents and becomes a
**tree rooted at the session's own main agent**. Under that root sit the two populations that are
today shown in two different places, or not at all: the **roster agents** attached to the session
(managed — the daemon runs their loop) and the session's **subagent sessions** (non-managed —
claude-cli and cursor sessions spawned as children). A subagent session nests its own subagents and
its own roster agents beneath it, so a spawn chain reads as a chain.

Every row carries the same status badge, drawn from the same vocabulary. A managed row reads
`SessionAgentEntry.status`; a non-managed row reads the **inferred** `SessionEntry.agent_status` the
daemon derives by tailing that session's conversation.

## Background

Three facts about the code as it stands make this change necessary rather than cosmetic.

**The Agents tab shows one of the two populations.** `SessionAgentRosterPane` renders a flat `<ul>`
over `StreamSessionAgents` — the specialized agents attached to one session
([docs/ft/daemon/session-agent-roster.md](../daemon/session-agent-roster.md)). It knows nothing about
subagent *sessions*.

**The other population is shown in the main pane, without status.** `SessionAgentsSection` lists the
selected session's peers (`orchestrator_session_id === this session`) with a `Switch` button and a
label derived from `SessionEntry.status` — the session's *lifecycle* string ("active"), which says
nothing about what the agent inside it is doing. The section sits in `SessionMainPane`, far from the
Agents tab that is named after exactly this subject.

**Neither surface expresses the relationship.** A subagent that spawned its own subagent renders as
a sibling of it. An operator reading five rows cannot tell which agent dispatched which.

### What the two upstream PRs already supply

This PR is the web consumer of two changes that are already on master, and it builds no new backend:

- **#410 — roster status.** `SessionAgentEntry.status` (11) and `last_activity` (12), with the
  `SessionAgentStatus` enum and its "UNSPECIFIED is *this daemon has nothing to say*, never idle"
  rule. The badge, the `data-agent-status` token and the ageing last-activity line already exist in
  `SessionAgentRosterPane` and are covered by `SessionAgentRosterStatus.cy.tsx`.
- **#419 — subagent conversation inference.** `SessionEntry.agent_status` (31) and
  `last_activity` (32), inferred by the daemon from `acp-transcript.jsonl`, the `AgentActivityHub`
  and the hook word, for `claude-cli` and `cursor-cli` sessions only
  ([docs/ft/daemon/agent-session-status.md](../daemon/agent-session-status.md)). The proto reuses
  `SessionAgentStatus` and `SessionAgentActivity` **verbatim**, with the stated intent that *"one
  badge renders a roster agent and a peer session alike"*. This PRD is where that badge becomes one
  badge.

⚠️ `packages/tddy-web/src/gen/connection_pb.ts` is **stale** with respect to #419: its `SessionEntry`
stops at field 30. The two inferred fields are not reachable from the web until it is regenerated,
which is why a regeneration is part of this change and not an incidental cleanup.

### What is deliberately not built

- **No transcript of what the main agent asked a roster agent.** PR #422's PRD records in detail why
  that is not buildable from the web today (a roster agent has no session directory, no transcript,
  and no conversation id on the wire). Nothing here revisits it: a roster row's observable history
  remains its `last_activity` summary.
- **No status for a subagent session the daemon does not tail.** `ensure_tailing` gates on
  `session_type ∈ {claude-cli, cursor-cli}`. A `tool` or `workspace` subagent is still a node in the
  tree — it was still spawned by this agent — and still reports `UNSPECIFIED`, which renders as
  "unknown". Inventing a status for it would be a claim no daemon made.
- **No new RPC.** The tree is assembled from two feeds that already exist: `ListSessions`, which the
  drawer already holds, and `StreamSessionAgents`, one per session node.

## Requirements

### Functional Requirements

#### The tree

- [x] **AC1** The Agents tab renders a **root row for the session's own main agent**, identified by
      the session's agent and model.
- [x] **AC2** Roster agents attached to the session render as **children of that root**, keeping the
      identity, host, clone-state, replaces and detach affordances they have today.
- [x] **AC3** Sessions whose `orchestrator_session_id` is this session render as **children of that
      root** as well.
- [x] **AC4** A subagent's own subagents nest **under it**, not under the root, to arbitrary depth.
- [x] **AC5** A subagent session's **own roster agents** render as its children when it is expanded.
- [x] **AC6** Sibling order is: roster agents in attach order, then subagent sessions in list order.
      Two runs over one session list produce one order.
- [x] **AC7** A session naming **itself** as its orchestrator is not rendered as its own child.
- [x] **AC8** An orchestrator **cycle** terminates, and no session is rendered twice within a branch.
- [x] **AC9** A session whose orchestrator is not the root and not a descendant of it does **not**
      appear in this tree.
- [x] **AC10** Depth is carried in the DOM (`data-depth`), not conveyed by indentation alone.

#### Status

- [x] **AC11** A **managed** row's badge is `SessionAgentEntry.status`, unchanged from today.
- [x] **AC12** A **non-managed** row's badge is `SessionEntry.agent_status`, rendered through the
      *same* names and `data-agent-status` tokens as a managed row.
- [x] **AC13** `UNSPECIFIED` renders as **"unknown"** on both kinds of row — never as "idle".
- [x] **AC14** A non-managed row shows `last_activity` as "&lt;summary&gt; · &lt;age&gt;", ageing on the
      pane's existing 60-second tick without a new frame.
- [x] **AC15** A row whose session or agent has no observed activity shows **no** last-activity line.
- [x] **AC16** Every row states which kind it is (`data-agent-kind` of `roster` or `session`), so
      "managed by this daemon" and "a session of its own" are never guessed from the label.

#### Actions

- [x] **AC17** A roster row keeps **Detach**, including the confirmation shown when detaching would
      delete a remote checkout.
- [x] **AC18** A subagent session row offers **Switch**, which reports that session's id to the host
      so the drawer can focus it.
- [x] **AC19** A subagent session row offers **no Detach** — there is no roster entry to detach.

#### Cost and routing

- [x] **AC20** A **collapsed** subagent session opens **no** roster stream.
- [x] **AC21** Expanding a subagent session reads its roster addressed to **its own** session and
      daemon — its *codebase half* when it is a split session, the rule the Inspector already applies
      to the root.
- [x] **AC22** The root is expanded when the tab opens; subagent sessions start collapsed.
- [x] **AC24** An expanded subagent whose roster **could not be read** says why, on its own row. An
      unreadable roster is not an empty one — the pane already refuses that conflation for the
      session it is about, and a subagent's host can be unreachable while the rest of the tree reads
      fine.

#### Removal

- [x] **AC23** `SessionMainPane` no longer renders the **Session agents** section. The peer list and
      its Switch action live in the Agents tab.

### Non-Functional Requirements

- [x] Pure web plus a **regeneration** of `src/gen/connection_pb.ts`. No `.proto` edit, no Rust.
- [x] The empty, loading, error and disconnected states of the pane are unchanged — a roster nobody
      could read must still never render as a roster with nothing in it.
- [x] No test-only branches in the tree: a node's shape is decided by the data, never by a flag the
      spec sets.

## Acceptance Test Outline

| # | Level | What it pins |
|---|---|---|
| AC7, AC8, AC9, AC6 | Unit (`bun test`) | The grouping is a pure list-to-tree fold: self-reference, cycles, orphans and sibling order are properties of the fold, and driving a cycle through a rendered tree proves nothing extra. |
| AC13, AC14 | Unit (`bun test`) | The status vocabulary and the relative-age text are pure string functions shared by both row kinds. |
| AC1-AC5, AC10-AC22, AC24 | Cypress component (pane) | The tree's subject is a rendered hierarchy reading two live feeds. Only a mount can show that a roster agent landed under the *right* parent, and only a mount can show a stream that was never opened. |
| AC18, AC23 | Cypress component (screen) | "Switch focuses that session" and "the old section is gone" span `SessionsDrawerScreen`, `SessionMainPane` and the Inspector — a narrower mount lets them disagree silently. |

## Open Questions

None. The three shape decisions — recursive tree over both populations, `SessionAgentsSection`
removed rather than left duplicating, and Switch on non-managed rows — were settled with the
developer before this PRD was written.
