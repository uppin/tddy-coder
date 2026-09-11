# Changeset: the session-agent and activity services

**Date**: 2026-09-09
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#unbundle` node **7 of 8**. PR [#476](https://github.com/uppin/tddy-coder/pull/476).
Base: `feature/unbundle/session-io-services` (node 6, PR #475)

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-09-09-unbundle-session-agent-services-initial-discovery.md](./2026-09-09-unbundle-session-agent-services-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or item dumps here.

## Responsibility

17 methods leave `connection.ConnectionService`, taking it from 50 to 33, plus 3,375 prod LoC.

| New coordinate | Family | Methods | Served by | Source that moves |
|---|---|---:|---|---|
| `session_agents.SessionAgentService` | B | 9 | `tddy-session-agents` *(new)* | `session_agent_clone.rs` (1,173), `session_agent_status.rs` (685), `session_agent_roster.rs` (605), `session_agent_inference.rs` (285) |
| `activity.ActivityService` | M, N | 8 | `tddy-session-activity` *(new)* | `session_notifications.rs` (433), `session_notification_subscribers.rs` (194) |

### The local socket keeps its surface

**Policy, set for the whole stack:** every family that was reachable on the local Unix socket as part
of `connection.ConnectionService` **stays reachable there after it moves**. That service carried all
90 methods on that socket, so any local caller — `tddy-sandbox-app` dials it today, and nothing
constrains callers outside this repo — could reach any of them. Dropping a family is a **silent
capability removal on a privileged local interface**, and its failure mode is a caller that used to
work receiving `unimplemented` with no announcement.

So this node puts both its services on the socket: `session_agents.SessionAgentService` (9 methods)
and `activity.ActivityService` (8) = **17 adapter methods**, matching node 1's 17 exactly.

They are **generated**, not hand-written, by the `generate_tonic_adapter` node 6 implements. That
makes node 6 a dependency on *behaviour* rather than on published surface — see `## Green wave`,
which moves this node to wave 4 as a result.

**This node has the widest consumer fan-out in the stack**, and one of those consumers is
security-relevant. `packages/tddy-sandbox-runner/src/runner.rs:89-94` holds the `(service, method)`
tuple allowlist of what an in-jail agent may relay to its host:

```rust
("connection.ConnectionService", "StreamSessionAgents" | "OpenAgentConversation"
    | "PromptAgentConversation" | "CancelAgentConversation" | "ReportAgentConversationState")
```

**Every one of those five is family B.** Moving the coordinate without moving the allowlist makes
every in-jail subagent conversation fail closed — the safe direction, but silently and at runtime
rather than at compile time. Updating it is a `## Scope` item and is tested **through a real jail**,
not by inspecting the list.

## Boundaries

This PR explicitly does **not**:

- Move `session_list_enrichment.rs` (1,740 LoC). It enriches `ListSessions`, which is family C and
  stays in the daemon deliberately.
- Take families A, L or P — node 8 — or C, D, O and Q, which stay.
- **Hand-write a tonic adapter.** If node 6's generator cannot produce one of these 17 methods, that
  is a gap in node 6 to report upward, not a licence to write 17 `async fn`s here — the whole reason
  the generator landed in node 6 is that hand-writing them is the cost being removed.
- Change what the sandbox relay allowlist *permits*. The set of allowed operations is identical; only
  the service name each tuple carries changes. Widening or narrowing the allowlist is out of scope
  and would hide a security change inside a mechanical one.
- Move the subagent conversation runtime. Node 5 put it in `tddy-discovery`; this node serves the
  conversation family in front of it.
- Change the `AgentActivityHub` broadcast semantics. It lives in `tddy-daemon-kernel` (node 1) and is
  consumed here unchanged.
- Force every file under 500 lines. `session_agent_clone.rs` (1,173), `session_agent_status.rs` (685)
  and `session_agent_roster.rs` (605) are over budget; split where cohesive, and whatever stays over
  is recorded in `## Scope`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` host-worktree-services | `move_module_to_crate`; `types.proto`; the proto-split pattern | `session_agents.proto` and `activity.proto` import `types.proto` for `SessionAgentEntry`, `SessionAgentRoster`, `AgentActivityRecord` and `StreamMode` | change `types.proto`'s shape |
| `n1` host-worktree-services | `tddy-daemon-kernel` exporting `AgentActivityHub` and `now_unix_ms` | `session_agent_inference.rs:36` imports the hub; the activity service publishes through it | re-define either symbol, or change the hub's broadcast semantics |
| **`n5` tools-thinning** | **`tddy-service`'s `SessionToolTransport` and roster client, and `tddy-discovery`'s subagent conversation runtime and `LiveAgentRoster`** | this node serves family B in front of both. Its `Draft PR contract` fixed those surfaces precisely so this node could compile against them | move, rename or reshape anything node 5 placed in `tddy-service` or `tddy-discovery` |
| **`n6` session-io-services** | **a working `generate_tonic_adapter`** in `tddy-codegen` — unary, server-streaming and bidirectional | both this node's services go on the local socket per the policy above, and their 17 adapter methods are generated rather than hand-written. This is a dependency on node 6's **behaviour**, not merely its published surface, which is what puts this node in wave 4 | implement or extend the generator; a gap found here is reported to node 6 |

`n6` is a branch ancestor, not a dependency — but **both edit
`packages/tddy-coder/src/session_participant/mod.rs`** (node 6 for family K, this node for families M
and N). Expect a conflict on every cascade and keep both dispatch changes.

## Draft PR contract

What lands in this PR's **second commit**:

- `packages/tddy-service/proto/session_agents.proto` (9 rpcs) and `activity.proto` (8 rpcs), both
  importing `types.proto`.
- `packages/tddy-session-agents/src/lib.rs` and `packages/tddy-session-activity/src/lib.rs` declaring
  each crate's entry constructor and trait ports (`SessionNotificationSubscriber`, the roster store)
  with real signatures, bodies annotated `// TODO(session-agent-services): implement`.
- The updated `(service, method)` tuples in `packages/tddy-sandbox-runner/src/runner.rs` — **pushed
  early precisely because it is the security-relevant edit**, so a reviewer sees it before the
  implementation lands around it.
- Both `Cargo.toml`s and their workspace `members` entries.
- The failing acceptance tests, including the through-a-real-jail conversation test.

**This is the first push of a PR that goes on to implement the same thing. It must never merge in
that state.**

## Green wave

**Wave:** **4 of 5** — moved from 3 by the local-socket policy.
**Greenable independently:** **not until node 6 is green.** Two reasons, and the second is new. Family
B is served in front of `tddy-discovery`'s conversation runtime and `tddy-service`'s roster client,
both of which node 5 puts there — that was always true. And under the reachability policy this node's
17 adapter methods are **generated** by node 6's `generate_tonic_adapter`, which is a dependency on a
predecessor's *behaviour*: the generator has to actually work, not merely be declared.
**Concurrent with:** node 8, which depends on node 6 for the same reason and shares this node's wave.
**Blocks:** nothing directly, but node 9 is now wave 5, behind this one.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4, n5      n2 → n4      n5 → n6, n7, n8      n6 → n7, n8      n7, n8 → n9

    w1  n1
    w2  n2, n3, n5
    w3  n4, n6
    w4  n7, n8        ← this node
    w5  n9

The local-socket policy is what deepened the graph from four waves to five: before it, nodes 7 and 8
needed nothing from node 6 and sat beside it in wave 3.

⚠ **Three recurring conflicts**: `packages/tddy-daemon/src/runtime.rs` (every node),
`packages/tddy-coder/src/session_participant/mod.rs` (nodes 6, 7, 8), and
`packages/tddy-sandbox-runner/src/runner.rs` (nodes 7 and 8 — node 8 owns the `ExecuteTool` tuple at
`:69`, this node owns the family-B tuples at `:89-94`).

## Affected Packages

- **tddy-session-agents** *(new)* — the 4 roster and clone modules
- **tddy-session-activity** *(new)* — the 2 notification modules; serves ACP replay
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md) — 6 modules and 3,375 prod LoC leave
  - [session-agent-roster.md](../../packages/tddy-daemon/docs/session-agent-roster.md), [agent-session-status.md](../../packages/tddy-daemon/docs/agent-session-status.md), [session-notifications.md](../../packages/tddy-daemon/docs/session-notifications.md) → the new crates' docs
  - [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md) — loses 17 endpoint entries
- **tddy-sandbox-runner**: the `(service, method)` relay allowlist
- **tddy-coder**: `src/session_participant/mod.rs` — families M and N **in lockstep**
- **tddy-service**: `session_agents.proto`, `activity.proto`; `connection.proto` loses 17 rpcs; the
  roster client node 5 moved here targets the new coordinate
- **tddy-discovery**: the subagent runtime node 5 moved here targets the new conversation coordinate
- **tddy-session-sync**: `StreamAgentActivityDelta`'s coordinate
- **tddy-sandbox-app**: its service-name guard
- **tddy-web**: 6 hooks, 2 components, 4 Cypress fakes

## Related Feature Documentation

- [PRD-2026-09-09-session-agent-services.md](../../ft/daemon/1-WIP/PRD-2026-09-09-session-agent-services.md)

## Summary

The session-agent roster and agent conversations become `session_agents.SessionAgentService`; agent
activity, session status, notifications and ACP replay become `activity.ActivityService`. Two new
crates take the source, and eight consumers — including the sandbox's relay allowlist — migrate in
the same PR.

## Background

See the PRD. The fact that shapes this node is that **family B is named in a security boundary**, not
just in clients. The sandbox relay allowlist is the list of operations an in-jail agent may perform
against its host, and five of its six entries are family B methods. That makes this the one node in
the stack where a missed consumer is not merely a broken feature but a silently disabled capability
inside a jail — which is why the allowlist update is pushed in the draft contract, ahead of the
implementation, and why its test drives a real jail rather than reading the list.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⛔ BLOCKING — `2026-08-29-a-session-s-first-delta-is-numbered-0-which-is-also-the-wire-s-no-tick.md`

A session's first activity delta is numbered 0, which is also the wire's "no tick" value, so a
consumer cannot distinguish "the first delta" from "no delta yet". `StreamAgentActivityDelta` moves to
a **new proto** in this PR, and every route around the entry is wrong: carrying the ambiguity into a
fresh schema makes it permanent and much harder to fix later, while leaving the field numbering alone
in the new proto means the new service ships a known-defective contract on day one. The new
`activity.proto` numbers ticks from 1 with an explicit unset, and `tddy-session-sync` — the only
consumer — migrates in the same PR. Earns a `## Scope` line.

### ⛔ BLOCKING — the `tddy-coder` lockstep requirement

Documented in `packages/tddy-coder/docs/changesets/2026-08-02-activities-tail-first-autoscroll.md`, and
argued at length in node 6's changeset. `tddy-coder`'s session participant serves families M and N
(`StreamAcpReplay` at 8 sites, `StreamSessionActivity`, `GetAcpToolCallDetail`, `GetAcpReplayPage`) and
must move in this same PR. Earns a `## Scope` line.

### ⚠ DURING — `2026-08-29-a-roster-agent-s-own-turns-are-unobservable-from-the-web.md`, `2026-08-30-the-agents-tab-tree-can-only-see-subagents-the-browser-already-lists.md`, `2026-08-19-session-creation-agent-catalog-the-rest-of-the-fan-out.md`, `2026-08-17-session-agent-roster-the-sandbox-bridge-and-other-deliberate-gaps.md`

Four open items inside the roster and conversation subsystem being moved. They survive the move
unchanged; recorded so a reviewer seeing them in `tddy-session-agents` knows they are inherited. None
is fixed here.

### ⚠ DURING — `2026-08-29-subagent-status-cannot-wait-for-a-turn-to-end.md`

In the conversation path. The move must not make the missing wait harder to add: `subagent_status`'s
readiness plumbing keeps its shape. Recorded, not fixed here.

### ℹ ANSWERED — `2026-08-13-docs-ft-daemon-background-tasks-md-claims-taskservice-unary-methods-pe.md`

Asks about a documentation claim regarding `TaskService`. Node 3 moved `task_service.rs` into
`tddy-task`; this node does not touch it. Re-read the entry against the post-node-3 layout at wrap.

## Scope

- [ ] **Proto**: `session_agents.proto` (9), `activity.proto` (8); `connection.proto` loses 17; vacated field numbers `reserved`
- [ ] **⛔ Delta tick numbering**: `activity.proto` numbers ticks from 1 with an explicit unset; `tddy-session-sync` migrated
- [ ] **`tddy-session-agents`**: crate, 4 modules, its suites
- [ ] **`tddy-session-activity`**: crate, 2 modules, ACP replay, its suites
- [ ] **Local socket**: both services adapted and `add_service`d on the one `Server::builder()`;
      17 methods, **generated** by node 6's `generate_tonic_adapter`, not hand-written
- [ ] **⛔ Sandbox relay allowlist**: the five family-B tuples re-pointed; permitted set unchanged
- [ ] **⛔ `tddy-coder` lockstep**: families M and N moved in this PR
- [ ] **Consumers**: `tddy-service`'s roster client, `tddy-discovery`'s runtime, `tddy-session-sync`, `tddy-sandbox-app`
- [ ] **Web**: 6 hooks, 2 components, 4 Cypress fakes migrated
- [ ] **File budget**: record which over-500-line files landed under budget and which did not, with why
- [ ] **Baseline**: `./test` per touched package back to the recorded numbers
- [ ] **Code Quality**: `cargo clippy -p <each> -- -D warnings` clean, `cargo fmt` clean
- [ ] **Documentation**: doc triage executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

`connection.ConnectionService` has 50 methods after nodes 1, 4 and 6. Families B (9), M (5) and N (3)
are among them. Eight consumers name their methods as strings, one of them a sandbox relay allowlist.
`tddy-coder`'s participant serves families M and N. A session's first activity delta is numbered 0,
colliding with the wire's "no tick".

### State B

`connection.ConnectionService` has **33 methods**. `session_agents.SessionAgentService` and
`activity.ActivityService` are served by two new crates on all transports and, for M and N, by
`tddy-coder`'s participant. The sandbox relay allowlist names the new services. Activity delta ticks
start at 1 with an explicit unset.

### Delta

#### tddy-service
- **Proto**: `session_agents.proto` and `activity.proto` added, importing `types.proto`;
  `connection.proto` loses 17 rpcs and the messages that move, vacated numbers `reserved`;
  the delta-tick field gains an explicit unset
- **Build**: one prost + one tonic pass per new service; both added to the descriptor set
- **Implementation**: the roster client node 5 moved here targets the new coordinate

#### tddy-daemon
- **Architecture**: 6 modules leave
- **Implementation**: two `ServiceEntry` groups move behind the new crates' constructors

#### tddy-sandbox-runner
- **Implementation**: the `(service, method)` tuples at `:89-94` name `session_agents.SessionAgentService`

#### tddy-coder
- **Implementation**: `session_participant/mod.rs`'s families M and N dispatch targets `activity.ActivityService`

#### tddy-session-sync
- **Implementation**: `StreamAgentActivityDelta`'s coordinate, and the tick-numbering change

#### tddy-web
- **Implementation**: 6 hooks, 2 components; the roster, conversation, replay and notification Cypress fakes split

## Implementation Milestones

- [ ] M1 — both protos generate; `types.proto` imported rather than duplicated; ticks start at 1
- [ ] M2 — the sandbox relay allowlist re-pointed; an in-jail conversation works through a real jail
- [ ] M3 — `tddy-session-agents` extracted; its suites pass
- [ ] M4 — `tddy-session-activity` extracted; ACP replay served; its suites pass
- [ ] M5 — `tddy-coder`'s participant moved; HTTP and LiveKit answer a session identically
- [ ] M6 — `tddy-service`, `tddy-discovery`, `tddy-session-sync`, `tddy-sandbox-app` migrated
- [ ] M7 — web migrated; Cypress component suites green; baselines restored; file budget recorded

## Testing Plan

**Primary test level: integration, per package.** The moved suites carry most of the proof —
`session_agent_remote_acceptance.rs` (1,697), `session_agent_roster_acceptance.rs` (738),
`session_agent_replacement_acceptance.rs` (594), `session_activity_delta_acceptance.rs` (671),
`agent_session_status_inference_*` and the notification suites all move with the code, unrewritten.

**The test that matters most drives a real jail.** The sandbox relay allowlist is a security boundary,
and an allowlist that no longer matches the served coordinate fails *closed* — so a test that
inspects the list would pass while every in-jail conversation was broken. The acceptance test spawns a
real sandboxed session and opens, prompts and cancels a subagent conversation from inside it. That is
the only assertion that distinguishes "the allowlist was updated" from "the allowlist was updated
correctly".

**The two-server parity test from node 6 is extended**, not duplicated: the same session is driven
over both participants for families M and N, asserting identical replay pages, tool-call details and
activity ordering.

Three further proofs:

- **`restructure verify --against <pre-move ref>`** from the repo root.
- **The moved-line diff**, alongside the visibility table.
- **A tick-numbering test** — a session's first delta is distinguishable from "no delta yet", which is
  the ⛔ prerequisite's acceptance criterion and cannot be asserted after the schema is frozen.

`tddy-web` keeps `mountWithRpc` + `anInMemoryRpcBackend`.

## Acceptance Tests

### tddy-session-agents
- [ ] **Integration**: all 9 `session_agents.SessionAgentService` methods answer on Connect-HTTP (`session_agent_service_acceptance.rs`)
- [ ] **Integration**: an agent attaches, is listed, is replaced and detaches across hosts (`session_agent_remote_acceptance.rs`)
- [ ] **Integration**: `StreamSessionAgents` reconnects with the documented backoff and `PASS_LONG_ENOUGH_TO_BE_SERVICE` (`session_agent_roster_acceptance.rs`)

### tddy-sandbox-runner
- [ ] **Integration**: an agent **inside a real jail** opens, prompts and cancels a subagent
      conversation through the updated relay allowlist (`in_jail_conversation_acceptance.rs`)
- [ ] **Unit**: the allowlist permits exactly the same operation set as before, under the new service name (`runner.rs`)

### tddy-daemon (the local socket)
- [ ] **Integration**: both `session_agents.SessionAgentService` and `activity.ActivityService` answer
      over the **local Unix socket**, not only over Connect-HTTP — through a real client on the real
      socket, since a mount left off answers `unimplemented` there and nowhere else (`local_token_uds.rs`)
- [ ] **Unit**: no hand-written `*_tonic_adapter.rs` is added by this node — both adapters are
      generated. Asserted by the adapters that exist, not by a test: a test reading source text or
      absent filenames passes in exactly the state it is meant to catch

### tddy-session-activity
- [ ] **Integration**: all 8 methods of families M and N answer (`activity_service_acceptance.rs`)
- [ ] **Integration**: ACP replay pages and tool-call details match the old coordinate's output (`acp_replay_parity_acceptance.rs`)
- [ ] **Unit**: a session's first delta tick is distinguishable from "no delta yet" (`activity_tick_unit.rs`)

### tddy-session-activity + tddy-coder
- [ ] **Integration**: the same session answers identically through the daemon and through
      `tddy-coder`'s participant for families M and N (`two_server_parity_acceptance.rs`)

### tddy-session-sync
- [ ] **Integration**: attach and stream activity deltas at the new coordinate, with the new tick numbering (`attach_acceptance.rs`, `sync_acceptance.rs`)

### tddy-web
- [ ] **Cypress component**: the agent roster pane attaches and detaches an agent (`SessionAgentRosterPane.cy.tsx`)
- [ ] **Cypress component**: the activity and replay panes render at the new coordinates (`AgentActivityPane.cy.tsx`)

### tddy-daemon
- [ ] **Integration**: `connection.ConnectionService` declares 33 methods and none of the 17 (`service_registration_acceptance.rs`)

## Decisions & Trade-offs

- **The local socket keeps its surface, and node 4 is now inconsistent with that.** The policy was
  set after node 4 had already **dropped** family T from the socket — there is no
  `livekit_tonic_adapter.rs`, and `local_socket_server.rs` still serves three services. Node 1
  preserved its families, node 4 did not, and nobody recorded either as a decision. Closing family
  T's gap is not this node's to do — it is a predecessor's file — so it is recorded in
  `docs/dev/todo/` where the inconsistency is visible rather than latent.
- **Families M and N share one service.** ACP transcript replay is arguably its own concern, but it is
  a *view* of agent activity, shares `AgentActivityRecord` and `StreamMode`, and three methods do not
  justify a fourth hand-written tonic adapter (one is needed per gRPC-served service, because the
  codegen's adapter generator is a stub).
- **The relay allowlist update is in the draft contract, ahead of the implementation.** It is the one
  security-relevant edit in the stack, and pushing it first means a reviewer sees it in isolation
  rather than buried in a 3,000-line move.
- **The tick-numbering fix is in scope, and that is a deliberate exception to "move only".**
  `StreamAgentActivityDelta` gets a new proto here, and a known-ambiguous field carried into a fresh
  schema becomes permanent. The fix is a field-numbering decision made once, with the single consumer
  migrated in the same PR — cheaper now than at any later point.
- **`session_list_enrichment.rs` stays** despite being the largest session module (1,740 LoC). It
  enriches `ListSessions`, which is family C. Taking it would mean taking family C, and the endpoint
  of this stack is a daemon that still owns session lifecycle.
- **Two crates rather than one.** Roster/conversations and activity/notifications could be one
  `tddy-session-agents`. They are two because their consumers differ sharply: family B is reached
  from inside jails through a security allowlist, families M and N from the web and from
  `tddy-session-sync`. One crate would give both consumer sets the union of the dependencies.

## Technical Debt & Production Readiness

- [ ] Four inherited roster/conversation gaps move unchanged; each is recorded in `## Prerequisites`
- [ ] `tddy-coder`'s session participant remains a string dispatch; the `docs/dev/todo/` entry node 6
      opens still stands
- [ ] Field numbers vacated in `connection.proto` are `reserved`, growing the schema's history

## Baseline

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-daemon` | **1027 passed / 1 failed**, 25 suites (inherited from node 1) | |
| `cargo clippy -p tddy-session-agents -p tddy-session-activity -p tddy-service --all-targets -- -D warnings` | ✅ exit 0 | |
| `./dev bun run --filter tddy-web cypress:component` | not yet run — no web change in commit 2 | |

**12 failing tests** define this node: 1 in `tddy-session-agents`, 3 in `tddy-session-activity`
(including the ⛔ tick-numbering prerequisite), and 8 in `tddy-service` — the inherited ones plus the
two new services' shape, `connection.ConnectionService` still declaring all 17, and **the sandbox
relay allowlist still gating family B under the old coordinate**.

### The allowlist is exposed as data, not repeated as strings

`tddy-session-agents` publishes `IN_JAIL_RELAYABLE` — the five `(service, method)` pairs — so
`tddy-sandbox-runner` reads them rather than repeating the literals. That is the mitigation for the
failure mode this changeset names: an allowlist that no longer matches the served coordinate fails
**closed**, silently, at runtime. With the pairs in one place they cannot drift.

### Two more shared types, found the same way

`types.proto` gains `SessionAgentStatus` and `SessionAgentActivity`, because `ListSessions` — family
C, which stays — reaches both through `SessionEntry`. Established by walking field types, exactly as
node 6 established `HostDocumentScope`. Families M and N share nothing with anything.

The known pre-existing failure inherited from node 1's baseline is expected to stay at exactly one.
Note that several in-jail suites need a real sandbox backend and are excluded from CI; their local
results are stated rather than claimed from a green build.

## Final Checklist

- [ ] `docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md` — the release-note file,
      carrying the 17 moved coordinates, the allowlist change and the tick-numbering fix
- [ ] Move `session-agent-roster.md`, `agent-session-status.md` and `session-notifications.md` to the
      new crates' `docs/`
- [ ] `packages/tddy-daemon/docs/connection-service.md` — remove 17 endpoint entries
- [ ] `docs/ft/web/agent-activity.md`, `agent-activity-pane.md`, `docs/ft/coder/specialized-subagents.md`,
      `docs/ft/daemon/remote-codebase-mode.md` — new coordinates and the new allowlist
- [ ] Close `docs/dev/todo/2026-08-29-a-session-s-first-delta-is-numbered-0-…md`
- [ ] Re-read the four inherited roster/conversation entries against the new crate
- [ ] Doc triage: `grep -rn -e 'StreamSessionAgents' -e 'OpenAgentConversation' -e 'StreamSessionActivity' -e 'StreamAcpReplay' packages/*/README.md packages/*/docs docs/ft`
