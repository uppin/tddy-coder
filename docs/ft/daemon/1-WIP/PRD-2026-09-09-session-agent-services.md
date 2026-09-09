# PRD: The session-agent and activity services

**Date**: 2026-09-09
**PRD Type**: Architecture Change (breaking RPC change — 17 methods)
**Product Area**: daemon
**Stack**: `#unbundle` node 7 of 8

## Affected Features

- [session-agent-roster.md](../session-agent-roster.md) — family B moves
- [agent-session-status.md](../agent-session-status.md) — status reporting moves
- [session-notifications.md](../session-notifications.md) — the notification stream moves
- [agent-activity.md](../../web/agent-activity.md), [agent-activity-pane.md](../../web/agent-activity-pane.md) —
  families M and N move; the panes' clients migrate
- [specialized-subagents.md](../../coder/specialized-subagents.md) — the conversation RPCs move
- [remote-codebase-mode.md](../remote-codebase-mode.md) — the in-jail relay allowlist changes
- [rpc-playground.md](../rpc-playground.md) — the picker gains two services, loses 17 methods

## Summary

17 of `ConnectionService`'s remaining 50 methods leave: the session-agent roster and agent
conversations (family B, 9) become `session_agents.SessionAgentService`, and agent activity, session
status, notifications and ACP transcript replay (families M and N, 8) become
`activity.ActivityService`. Two new crates, `tddy-session-agents` and `tddy-session-activity`, take
3,375 prod LoC of source with them.

## Background

This node has the **widest consumer fan-out in the stack**, and that is the whole reason it is planned
rather than improvised. Family B's five most-used methods are named in a security-relevant allowlist
inside the sandbox:

```rust
// packages/tddy-sandbox-runner/src/runner.rs:89-94
("connection.ConnectionService", "StreamSessionAgents" | "OpenAgentConversation"
    | "PromptAgentConversation" | "CancelAgentConversation" | "ReportAgentConversationState")
```

That tuple allowlist is what an in-jail agent may relay to its host. Move the coordinate without
moving the allowlist and every subagent conversation from inside a jail fails closed — which is the
safe direction, but silently and at runtime rather than at compile time.

Five further consumers name these methods as strings: `tddy-tools` (7 of them, reaching the daemon
from inside a jail), `tddy-session-sync` (`StreamAgentActivityDelta`), `tddy-sandbox-app`,
`tddy-discovery`, and `tddy-coder`'s session participant — which serves families M and N itself and
must move in lockstep for the same reason node 6 documents.

## Proposed Changes

### What changes

| New coordinate | Family | Methods | Served by |
|---|---|---:|---|
| `session_agents.SessionAgentService` | B | 9 | `tddy-session-agents` *(new)* |
| `activity.ActivityService` | M, N | 8 | `tddy-session-activity` *(new)* |

Source that moves: `session_agent_clone.rs` (1,173), `session_agent_status.rs` (685),
`session_agent_roster.rs` (605), `session_agent_inference.rs` (285) to `tddy-session-agents`;
`session_notifications.rs` (433) and `session_notification_subscribers.rs` (194) to
`tddy-session-activity`.

Both crates serve through the surfaces node 5 put in place: `tddy-service`'s
`SessionToolTransport` and roster client, and `tddy-discovery`'s subagent conversation runtime and
`LiveAgentRoster`.

### What stays the same

- Every roster, conversation, activity and replay behaviour. `restructure verify --against <ref>`
  proves the statement multiset is unchanged.
- `session_list_enrichment.rs` (1,740 LoC) stays in the daemon — it enriches `ListSessions`, which is
  family C.
- The remaining 33 `ConnectionService` methods, until node 8.

## Impact Analysis

### Technical

| Area | Impact |
|---|---|
| `tddy-daemon` | −6 modules, −3,375 prod LoC; two `ServiceEntry` groups |
| `tddy-sandbox-runner` | **the `(service, method)` relay allowlist changes** — the security-relevant edit in this node |
| `tddy-coder` | its family M and N dispatch moves **in lockstep** |
| `tddy-tools` / `tddy-service` | 7 method-name literals move (post-node-5 they live in `tddy-service`) |
| `tddy-session-sync` | `StreamAgentActivityDelta`'s coordinate |
| `tddy-discovery` | the subagent runtime (moved there by node 5) targets the new conversation coordinate |
| `tddy-service` | `session_agents.proto` and `activity.proto` appear; `connection.proto` loses 17 rpcs |
| `tddy-web` | `useSessionActivity`, `useSessionAgentRoster`, `useAgentConversation`, `useAcpReplay`, `useAcpToolCallDetail`, `useSessionNotifications`, `SessionAgentRosterPane`, `SessionMainPane`, plus 4 Cypress fakes |

### User-facing

17 RPC coordinates move. A web bundle from before this node cannot attach an agent, open a
conversation, or render the activity and replay panes.

## Implementation Plan

1. `session_agents.proto` and `activity.proto` created, importing `types.proto`.
2. `tddy-session-agents` and `tddy-session-activity` extracted.
3. **The sandbox-runner allowlist updated in the same commit as the proto change.**
4. `tddy-coder`'s participant moved for families M and N.
5. `tddy-tools`/`tddy-service`, `tddy-session-sync`, `tddy-sandbox-app` and `tddy-discovery` migrated.
6. `tddy-web` migrated; the four Cypress fakes split.

## Acceptance Criteria

- [ ] `session_agents.SessionAgentService` serves all 9 family-B methods on all transports
- [ ] `activity.ActivityService` serves all 8 methods of families M and N
- [ ] `connection.ConnectionService` declares none of the 17, and is down to 33 methods
- [ ] **an in-jail agent opens, prompts and cancels a conversation through the updated relay allowlist** —
      tested through a real jail, not by inspecting the allowlist
- [ ] `tddy-coder`'s participant serves families M and N at the new coordinates, and a session reached
      over LiveKit and over HTTP answers identically
- [ ] `tddy-session-sync` attaches and streams activity deltas at the new coordinate
- [ ] `tddy-web` attaches an agent, opens a conversation, and renders the activity and replay panes
- [ ] `./test -p tddy-daemon -p tddy-session-agents -p tddy-session-activity -p tddy-coder -p tddy-session-sync`
      matches baseline

## References

- Changeset: [2026-09-09-unbundle-session-agent-services.md](../../../docs/dev/1-WIP/2026-09-09-unbundle-session-agent-services.md)
- Discovery: [2026-09-09-unbundle-session-agent-services-initial-discovery.md](../../../docs/dev/1-WIP/2026-09-09-unbundle-session-agent-services-initial-discovery.md)
- Node 6's PRD, for the lockstep argument: [PRD-2026-09-09-session-io-services.md](./PRD-2026-09-09-session-io-services.md)
