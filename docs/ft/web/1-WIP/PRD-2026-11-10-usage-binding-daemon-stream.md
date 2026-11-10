# The daemon accounts every session's usage, and the Usage tab shows all of it - PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#usage-binding 2/5` — branch `feature/usage-binding/daemon-stream`, on `#usage-binding 1/5`

## Affected Features

- **Primary Feature**: [Session Usage Inspector](../session-usage-inspector.md) — where the stream
  comes from, which sessions have it, the columns and the conversation tree.
- **Related Feature**: [Session Token Accounting](../../coder/session-token-accounting.md) — the
  daemon, not tddy-coder, runs the accounting for every session.

## Summary

The Session Inspector's **Usage** tab shows real per-conversation usage for **every** session the
daemon runs:
- host terminal (PTY) sessions;
- jailed terminal sessions;
- workflow sessions.

The daemon owns one usage watcher per session. It resolves where the agent's files live from the
agent's spec:
- a jailed agent uses its jail home, e.g. `<tddyhome>/homes/claude`;
- a host agent uses the OS user's home.

The daemon serves the snapshot as `ActivityService.StreamSessionUsage`: the current snapshot first,
then every change. The tab reaches it the way every other Inspector tab reaches the daemon, with no
LiveKit room and no presenter.

The table gains columns: **In · Cache read · Cache write · Out · Reasoning · Total · Cost · Turns**. Rows nest as a
tree: a subagent sits indented under the conversation that spawned it, at any depth.

## Background

- Today only `tddy-coder` runs a usage watcher, and only when it joined LiveKit. It serves the stream
  over `TddyRemote.Stream`.
- Terminal sessions have no `TddyRemote` server, so their tab is always empty. One example is session
  `01a1c71c-…`, whose transcript exists on disk.
- The watcher also reads `$HOME`. A jailed Claude writes its transcripts under `<tddyhome>/homes/claude`,
  so even a jailed workflow session reads the wrong place.

## Proposed Changes

### What's Changing

- **Every launch records its usage context.** The host PTY, jailed and workflow launch paths, and
  their resume and relaunch paths, write `<session_dir>/usage-context.json` at launch:
  - the agent spec id;
  - `agent_home`, as a host path: the jail's home for a jailed session, the OS user's home for a host
    session;
  - `conversation_id`;
  - `worktree`.
- **The daemon evaluates usage while a client watches.** On subscribe, the daemon reads that context,
  takes the spec's `usage:` bindings (`#usage-binding 1/5`) from its registry, and evaluates them with
  `{session_dir}`. It appends the tddy subagent rows from `egress/accounting.json`. While at least one
  client is subscribed it re-evaluates on a poll tick and broadcasts the snapshot when it changes. One
  poller is shared per session.
- **Live and ended sessions are handled alike.** An ended session is evaluated from the same files;
  its snapshot simply stops changing. A session that predates this change has no context file and
  streams only its tddy subagent rows.
- **A new RPC, `ActivityService.StreamSessionUsage(session_id)`.** It returns a stream of
  `SessionUsageSnapshot { repeated ConversationRecord conversations }`: the snapshot on subscribe, then
  each change. A session owned by a peer daemon is routed to its owner, as the other Inspector streams
  are.
- **The Usage tab subscribes through the daemon client**, the same one the Activity tab uses.
- **New columns in the Usage tab**: In, Cache read, Cache write, Out, Reasoning, Total, Cost, Turns.
  - Cost shows `$x.xx` when the source reported a cost, and is blank otherwise.
  - The TOTAL row sums every column. Cost is summed only over the rows that have one, and it is marked
    partial when some rows have none.
- **Rows are a tree.** A conversation with a `parent_id` renders under its parent, indented by depth.
  A child whose parent is missing renders at the top level.
- **Removed:**
  - tddy-coder's usage watcher;
  - `TddyRemote`'s `TokenUsageUpdated` event;
  - the presenter's `TokenUsageUpdated` event;
  - the presenter `usage_watcher` module.

  The daemon is the only source.

### What's Staying the Same

- What a conversation row is, and the bindings that produce it (`#usage-binding 1/5`).
- The tab's empty and stream-error states and its test ids.
- Snapshots stay whole and cumulative, never deltas.

## Impact Analysis

### Technical Impact

- **The proto** (`activity.proto`) gains the RPC and its messages. `ConversationRecord` moves with it
  out of `remote.proto`.
- **The daemon side**:
  - `tddy-session-activity` serves the stream;
  - the session launch (`tddy-agent-launch`) records each session's resolved agent-home and binding
    inputs, so a watcher can be started at launch, resume and relaunch;
  - a dormant session's inputs are recomputed from its metadata and spec.
- **`tddy-session-web`**: `useSessionUsage`, `SessionUsageTab` and `sessionUsage.ts`, plus the Cypress
  backend and its fixtures.
- **`tddy-coder` / `tddy-presenter` / `tddy-service`**: the TddyRemote usage path is deleted.

### User Impact

- Terminal sessions show usage for the first time.
- New columns, and a nested view of the conversations.
- The Usage tab no longer needs LiveKit.

## Implementation Plan

1. The usage context, written by every launch path (host PTY, jail, workflow) and its resume and
   relaunch paths.
2. A per-session poller in the daemon, shared by the session's subscribers.
3. The `StreamSessionUsage` RPC, with peer routing.
4. The web transport switch, then the columns and the tree.
5. Delete the TddyRemote / presenter usage path.
6. Tests:
   - a daemon acceptance test: start a host terminal session whose agent spec is bound to a fixture
     transcript, subscribe, see the snapshot, append to the transcript, see the change;
   - the jailed home resolution;
   - a dormant snapshot;
   - Cypress component tests for the columns, cost, the tree and the transport.

## Acceptance Criteria

- [ ] A host terminal session's Usage tab shows its main conversation and its subagents, with
      In / Cache read / Cache write / Out / Reasoning / Total / Cost / Turns.
- [ ] A jailed session reads its agent's files from the jail home, not the daemon's or user's `$HOME`.
- [ ] Usage updates live while the session runs (snapshot on subscribe, then every change).
- [ ] An ended session's Usage tab shows its final usage.
- [ ] Subagents nest under their parent at any depth.
- [ ] No usage path remains in tddy-coder / presenter / `TddyRemote`.

## References

### Affected Features (Complete List)
- [Session Usage Inspector](../session-usage-inspector.md) — transport, columns, tree, every session.
- [Session Token Accounting](../../coder/session-token-accounting.md) — the daemon runs accounting.

### Related Documentation
- Parent: `#usage-binding 1/5` (`feature/usage-binding/claude-transcript`).
