# Changeset: Session agent roster — attach any number of agents, from any daemon

**Date**: 2026-08-16
**Status**: 🚧 In Progress
**Type**: Feature
**PRD**: [docs/ft/daemon/session-agent-roster.md](../../ft/daemon/session-agent-roster.md)
**Branch**: `feat-session-agents-revamp`

## Affected Packages

- **tddy-service**: [README.md](../../../packages/tddy-service/README.md)
  - `connection.proto` — `SessionAgentRoster` / `SessionAgentEntry`, `AttachSessionAgent`,
    `DetachSessionAgent`, `ListSessionAgents`, `StreamSessionAgents`, the three
    `*AgentConversation` RPCs, `SubagentInfo` fields 4-7, `StartSessionRequest.specialized_agents`
    semantics (qualified ids)
  - `session_agents.rs` (new) — the `session.agents` broadcast topic constant, beside
    `session_activity.rs`
  - [changesets.md](../../../packages/tddy-service/docs/changesets.md)
- **tddy-core**: [README.md](../../../packages/tddy-core/README.md)
  - `session_metadata.rs` — `agents: Vec<SessionAgentRecord>` + `agents_rev: u64` **replacing**
    `specialized_agents: Vec<String>`
  - `session_agent.rs` (new) — `SessionAgentRecord`, `AgentId` (parse/format `name@daemon`)
  - [changesets.md](../../../packages/tddy-core/docs/changesets.md)
- **tddy-discovery**: [README.md](../../../packages/tddy-discovery/README.md)
  - `agent_def.rs` — `builtin_fastcontext_def` / `builtin_agent_defs` deleted
  - `subagent.rs` — `subagent_replaced_tools` / `resolve_replaced_tools` deleted
  - `backend.rs` — `FastContextBackend` → `SpecializedAgentBackend`
  - [changesets.md](../../../packages/tddy-discovery/docs/changesets.md)
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md)
  - `session_agent_roster.rs` (new) — the store, revisioning, persistence
  - `session_agent_clone.rs` (new) — remote clone provisioning + the in-process mirror
  - `connection_service.rs` — the four roster RPCs, the three conversation RPCs **and their
    routing** (no separate `session_agent_conversation.rs` was created; `AgentConversation` and its
    routing live here), remote def resolution, `roster_replacement_pairs`, the clone teardown paths
  - `session_room.rs` — the `session.agents` publisher and `seed_uncommitted_state`. **Not** an
    admission handshake: the owning daemon self-mints its room token (see § Deviations)
  - `session_deletion.rs` / `worktrees.rs` — tearing down every remote clone
  - `workspace_session.rs` — `start_agent_clone_session`
  - ⚠️ `packages/tddy-daemon/docs/session-agent-roster.md` was planned and **not written** — the
    module doc this changeset promised does not exist yet
  - [changesets.md](../../../packages/tddy-daemon/docs/changesets.md)
- **tddy-tools**: [README.md](../../../packages/tddy-tools/README.md)
  - `session_agents.rs` (new — *not* `session_agent_roster_client.rs` as originally planned) —
    `LiveAgentRoster`, `RosterCurrency`, and the `StreamSessionAgents` follower with reconnect
  - `server.rs` / `main.rs` / `action_tools.rs` — registry from roster, a hand-written `list_tools`
    gated on the live roster, `tools/list_changed`, no `TDDY_SUBAGENT`, runtime refusal of a
    replaced tool, retired-conversation accounting
  - `session_tool_client.rs` — `connect_sandbox_ipc` extracted so the roster stream gets its own
    connection
  - [changesets.md](../../../packages/tddy-tools/docs/changesets.md)
- **tddy-sandbox**: `context_dir.rs` — appendix rendered from the roster's qualified ids
- **tddy-sandbox-recipes**: `claude_cli.rs` — `build_claude_allowlist` / `build_claude_disallowlist`
  from the persisted roster
- **tddy-sandbox-app**: `config.rs` / `spawn.rs` / `main.rs` — action-author + coder validation
  removed; no builtin to fall back on
- **tddy-coder**: `run.rs` — `--fastcontext-*` flags deleted; `session_participant/mod.rs` — roster
  for coder-hosted sessions
- **tddy-web**: `components/sessions/SessionAgentRosterPane.tsx` (new),
  `components/sessions/useSessionAgentRoster.ts` (new — placed beside its consumer, matching
  `useChildSessions.ts`, not in `src/hooks/`), `components/sessions/useAvailableAgents.ts` (new —
  the `ListSubagents` fan-out, modelled on `useModelRegistryFanOut.ts`),
  `CreateSessionPane.tsx` (fanned-out picker, qualified ids), `InspectorTabs.tsx` +
  `SessionInspectorDrawer.tsx` + `routing/appRoutes.ts` (the Agents tab), `cypress/support/testIds.ts`

## Related Feature Documentation

- [Session agent roster](../../ft/daemon/session-agent-roster.md) — this feature
- [Specialized subagents](../../ft/coder/specialized-subagents.md) — the model superseded
- [Session rooms](../../ft/daemon/session-room.md) — the room remote daemons join
- [Session worktree sync](../../ft/daemon/session-worktree-sync.md) — the sync a clone runs
- [Remote managed worktree](../../ft/daemon/remote-managed-worktree.md) — workspace session + proxy
- [Models & Agents](../../ft/web/models-and-agents.md) — the registry def source

## Summary

Turns the session's specialized agents from a **fixed list of names frozen at spawn** into a
**revisioned roster mutated on a live session**, addressable across daemons, each remote daemon
serving its agents from its own independently-synced clone. Deletes every hardcoded agent and every
hardcoded per-tool meaning of `replaces`.

## Scope

**In scope**

- The roster: wire, store, persistence, revisioning, broadcast, live registry rebuild.
- Attach/detach at runtime, including remote daemons and their clones.
- Qualified `name@daemon_instance_id` ids everywhere an agent is named.
- Removal of `builtin_fastcontext_def`, `builtin_agent_defs`, `subagent_replaced_tools`,
  `resolve_replaced_tools`, `FastContextBackend`'s name, the `--fastcontext-*` flags, the
  `TDDY_SUBAGENT` default, and the action-author/coder `replaces` semantics.
- Runtime refusal of a replaced tool, so live attach is enforceable.
- Web: fanned-out picker + Agent roster pane.

**Out of scope** (PRD § Non-goals, plus)

- Write-back from a remote clone; a remote `SHELL` still runs on the facilitating daemon.
- Autonomous agents, agent-to-agent addressing, multi-hop routing.
- Migrating `specialized_agents` out of existing `.session.yaml` files.
- Per-agent clone isolation on one host.

## Technical Changes

### State A (Current)

- **`SessionMetadata.specialized_agents: Vec<String>`** (`session_metadata.rs:66`) — bare names,
  written once at start, never mutated.
- **`StartSessionRequest.specialized_agents = 18`** (`connection.proto:472`) — bare names, resolved
  by `ConnectionServiceImpl::resolve_specialized_agent_defs` (`connection_service.rs:2665`) against
  `resolvable_agent_defs()` (`:2605`) = **builtins ∪ `<tddyhome>/agents/*.yaml` ∪ registry
  assistants**, all three **local to the facilitating daemon**. An unknown name is
  `invalid_argument`.
- **`specialized_subagent_env`** (`connection_service.rs:2689`) serializes the resolved defs into
  `TDDY_SUBAGENT` (comma names) + `TDDY_SUBAGENTS_JSON` (JSON array), injected into the jail.
- **`tddy-tools`** rebuilds `SubagentRegistry::from_defs(subagents_from_env())` **per call**
  (`server.rs:1765`, `action_tools.rs:117`) — so it re-reads the env each time, but the env never
  changes. `subagent_new_session` falls back to `TDDY_SUBAGENT` when `agent` is absent
  (`server.rs:1747-1753`).
- **`replaces`** (`agent_def.rs:142`) is normalized by `normalize_replaced_tools`
  (`subagent.rs:465`) and unioned by `resolve_replaced_tools_for_defs` (`:518`).
  `subagent_replaced_tools` (`:486`) hardcodes a `"fastcontext"` arm; `resolve_replaced_tools`
  (`:504`) adds a CSV override.
- **Hardcoded per-tool roles** in `tddy-sandbox-app/src/config.rs:143-176`: replacing `Shell` ⇒
  action author (at most one def), replacing `Write`/`StrReplace`/`Delete` ⇒ coder, and "the def
  must bind the matching internal tool or the session is rejected".
- **`builtin_fastcontext_def()`** (`agent_def.rs:169`) and **`builtin_agent_defs()`** (`:187`) —
  `resolve_agent_defs(dir)` (`:227`) seeds from them, so an empty dir still yields `fastcontext`.
- **`FastContextBackend`** (`tddy-discovery/src/backend.rs:27`), `name()` returning
  `"fastcontext"`, defaults `microsoft/FastContext-1.0-4B-RL` + `http://localhost:30000`; still
  reachable through `--fastcontext-url` / `--fastcontext-model` / `--fastcontext-max-turns`
  (`run.rs:788-800`).
- **`ListSubagents`** (`connection.proto:263-271`) returns `{name, label, model}` — **no daemon
  stamp**, so a fanned-out list is ambiguous.
- **`CreateSessionPane`** calls `listSubagents({})` on **one** daemon (`:282`) and sends bare names
  (`:501`, `:528`).
- **Session room** (`session-room.md`) already exists, hosted by the facilitating daemon; its
  membership table anticipates "further agents … minting a second token; no daemon-side
  registration".
- **Split placement** already provides `workspace` sessions with caller-chosen
  `requested_session_id`, `StreamExecuteTool` tool proxying, and the teardown discipline.
- **`tddy-session-sync`** already provides `StreamAgentActivityDelta`, `StreamReadWorktreeFile`,
  `refs/tddy/session/{id}/wip` and the mirror algorithm — as a standalone binary.
- **`SessionAgentsSection.tsx`** already exists and means **peer child sessions** — an unrelated
  concept with a colliding name.

### State B (Target)

- The roster is the single source of truth for which agents a session has, on the wire
  (`SessionAgentRoster`, revisioned), at rest (`.session.yaml` `agents` / `agents_rev`), and in the
  in-jail registry (rebuilt from `StreamSessionAgents`).
- Every agent is `name@daemon_instance_id`. `ListSubagents` stamps the daemon; the picker fans out;
  the roster routes off the id.
- A remote agent's def is resolved on its owning daemon, its loop runs there against a clone in a
  `workspace` session kept current by the sync algorithm in-process, and its mutating tools proxy
  back to the facilitating daemon.
- `replaces` is a plain union withdrawn from the main agent, enforced at spawn (allowlist) and at
  call time (`tddy-tools` refusal).
- No builtin agent def, no `fastcontext` identifier, no `TDDY_SUBAGENT` default, no
  `--fastcontext-*` flags, no action-author/coder roles.

### Delta

#### tddy-service

```proto
// connection.proto — additions

service ConnectionService {
  rpc AttachSessionAgent(AttachSessionAgentRequest) returns (SessionAgentRoster);
  rpc DetachSessionAgent(DetachSessionAgentRequest) returns (SessionAgentRoster);
  rpc ListSessionAgents(ListSessionAgentsRequest) returns (SessionAgentRoster);
  rpc StreamSessionAgents(StreamSessionAgentsRequest) returns (stream SessionAgentRoster);

  // Conversation routing — `tddy-tools` calls these on its facilitating daemon; the daemon runs a
  // local entry in-process and forwards a remote entry to its owning daemon in the session room.
  rpc OpenAgentConversation(OpenAgentConversationRequest) returns (OpenAgentConversationResponse);
  rpc PromptAgentConversation(PromptAgentConversationRequest) returns (stream AgentConversationChunk);
  rpc CancelAgentConversation(CancelAgentConversationRequest) returns (CancelAgentConversationResponse);
}

message SessionAgentRoster {
  string session_id = 1;
  uint64 rev = 2;
  repeated SessionAgentEntry agents = 3;
}

message SessionAgentEntry {
  string agent_id = 1;              // "explorer@ws-01"
  string name = 2;
  string daemon_instance_id = 3;
  string label = 4;
  string model = 5;
  repeated string replaces = 6;     // snapshotted at attach
  repeated string tools = 7;        // snapshotted at attach
  string codebase_session_id = 8;   // the clone's workspace session; empty when local
  AgentCloneState clone_state = 9;
  string clone_error = 10;
}

enum AgentCloneState {
  AGENT_CLONE_STATE_UNSPECIFIED = 0;
  AGENT_CLONE_STATE_LOCAL       = 1;  // the owning daemon is the facilitating daemon
  AGENT_CLONE_STATE_PROVISIONING= 2;
  AGENT_CLONE_STATE_READY       = 3;
  AGENT_CLONE_STATE_ERROR       = 4;
}

message SubagentInfo {
  string name = 1;
  string label = 2;
  string model = 3;
  string daemon_instance_id = 4;    // NEW
  string agent_id = 5;              // NEW
  repeated string replaces = 6;     // NEW
  repeated string tools = 7;        // NEW
}
```

`PromptAgentConversation` is **server-streaming** for the same reason `StreamExecuteTool` is: a
subagent's answer has no useful upper bound and a payload over `MAX_CHUNK_FRAME_BYTES` (60 000) is
chunk-framed, where a lost frame wedges the call with no error. Frames cap at 48 KiB, matching
`HOST_DOCUMENT_FRAME_BYTES`.

Broadcast topic, beside `session.activity` and `worktree.activity`:

```
session.agents  →  binary connection.SessionAgentRoster
```

#### tddy-core

```rust
// session_agent.rs (new)

/// A roster agent's qualified id: `name@daemon_instance_id`. A `name` containing `@` is refused,
/// so a formatted id always parses back to the pair it was built from.
pub struct AgentId { pub name: String, pub daemon_instance_id: String }
impl AgentId {
    pub fn parse(s: &str) -> Result<Self, AgentIdError>;
    pub fn qualified(&self) -> String;   // "name@daemon"
}

/// One roster entry as persisted in `.session.yaml`.
pub struct SessionAgentRecord {
    pub agent_id: String,
    pub name: String,
    pub daemon_instance_id: String,
    pub label: Option<String>,
    pub model: String,
    pub replaces: Vec<String>,
    /// Exec-catalog tool names. `Vec<String>`, not `Vec<SubagentTool>`: `tddy-discovery` depends
    /// on `tddy-core`, so the enum is not nameable here — and the wire carries strings anyway.
    pub tools: Vec<String>,
    pub codebase_session_id: Option<String>,
}
```

```rust
// session_metadata.rs — replaces `specialized_agents`
#[serde(default)] pub agents: Vec<SessionAgentRecord>,
#[serde(default)] pub agents_rev: u64,
/// Tombstone. `SessionMetadata` is `deny_unknown_fields`, so removing the key outright would make
/// every pre-roster `.session.yaml` fail to parse — the opposite of AC11. Read and discarded,
/// never written back, never consulted.
#[serde(default, skip_serializing)] pub legacy_specialized_agents: Vec<String>,
```

#### tddy-daemon

- `session_agent_roster.rs` — `SessionAgentRosterStore`: per-session roster, monotonic `rev`,
  `attach` (idempotent on `agent_id`), `detach`, `snapshot`, `subscribe` (a `broadcast` channel
  whose subscriber receives the current snapshot first), persistence through
  `tddy_core::atomic_file`, and publication on `session.agents`.
- `session_agent_clone.rs` — `ensure_clone(session, owning_daemon)`: mint the workspace session id,
  forward `StartSession { session_type: "workspace", requested_session_id }` to the peer with the
  split forward's extended timeout, then run the sync loop in-process on the *owning* daemon.
  `release_clone` on the last detach, with the split teardown's idempotency rule.
- `session_agent_conversation.rs` — `open` / `prompt` / `cancel`, dispatching on whether the entry's
  `daemon_instance_id` is local; remote calls ride `forward_server_stream_to_peer` in the session
  room.
- `connection_service.rs` — the seven new handlers; `resolvable_agent_defs` stamps
  `daemon_instance_id`; a new `resolve_qualified_agent_id` that fans out to a peer's
  `ListSubagents` when the id names one; `resolve_specialized_agent_defs` takes qualified ids.
- `session_room.rs` — `admit_owning_daemon` (mint a scoped token, hand it to the peer, track
  membership so the last detach can revoke it); publish `session.agents` on every `rev` change.
- `session_deletion.rs` — delete every `codebase_session_id` in the roster before removing the
  session directory.

#### tddy-tools

- `session_agent_roster_client.rs` — opens `StreamSessionAgents` on the detected transport, holds
  it for the process lifetime with reconnect-on-drop and backoff, exposes
  `current() -> SessionAgentRoster` and a change notifier. **Its own connection**, because
  `SandboxIpc` opens a fresh `UnixStream` per dispatch today.
- `server.rs` — the registry is built from `roster.current()` rather than `subagents_from_env()`;
  a roster change emits `notifications/tools/list_changed`; `subagent_new_session` without `agent`
  errors listing the roster's ids; a remote entry dispatches `OpenAgentConversation` /
  `PromptAgentConversation` / `CancelAgentConversation` instead of constructing a local session.
- `execute_tool` dispatch — a tool name in the roster's replaced union is refused with a message
  naming the replacing agent's qualified id.

#### tddy-sandbox / tddy-sandbox-recipes / tddy-sandbox-app

- `context_dir.rs` — the appendix lists the roster's qualified ids and each agent's replaced set.
- `claude_cli.rs` — `build_claude_allowlist` / `build_claude_disallowlist` read the persisted
  roster.
- `tddy-sandbox-app/src/config.rs` — `validate_subagent_roles` (action author / coder / must-bind)
  deleted; `resolve_specialized_agents` resolves only against `<tddyhome>/agents`.

#### tddy-coder

- `run.rs` — `--fastcontext-url` / `--fastcontext-model` / `--fastcontext-max-turns` and their
  `Config` fields deleted; `create_backend` builds `SpecializedAgentBackend` from the def alone.
- `session_participant/mod.rs` — coder-hosted sessions (tool, cursor-cli) subscribe to the roster
  so they are not a blind spot, as they already do for activity reporting.

#### tddy-web

- `useSessionAgentRoster.ts` — subscribes to `StreamSessionAgents` on the session's daemon.
- `SessionAgentRosterPane.tsx` — the roster pane; four distinct states; add/detach.
- `useAvailableAgents.ts` — fans `ListSubagents` out across common-room daemons, per-daemon error
  isolation, returns qualified ids.
- `CreateSessionPane.tsx` — the multi-select consumes `useAvailableAgents` and sends qualified ids.

## Design decisions

### The roster is a snapshot, never a diff

The consumer that matters is an MCP registry whose disagreement with the daemon is *silent*: it
answers `subagent_new_session` for a detached agent and refuses an attached one. A snapshot cannot
drift; a missed diff can. `rev` exists to detect staleness, not to reconstruct state.

### `replaces` and `tools` are frozen into the entry at attach

Re-reading the def on every use would let an edit to a YAML file or a registry assistant silently
change what a running session's main agent may call. Freezing makes the roster an audit of what was
agreed, and makes detach/re-attach the explicit way to pick up an edit.

### Runtime refusal, not relaunch

`--allowedTools` is fixed when `claude` spawns. Relaunching to withdraw a tool would interrupt the
conversation the operator attached the agent in the middle of. Refusing at the call site works on
the path the call already takes — and in a managed session that path *is* `tddy-tools`. The
consequence is stated rather than hidden: attaching a `replaces`-carrying agent to a **non-managed**
session is refused, because there the main agent's native tools never reach `tddy-tools`.

### One clone per (session, remote daemon)

Two agents on one host reading the same tree is the common case. A checkout each multiplies disk and
sync cost for isolation a read-only mirror does not need.

### The owning daemon joins the room; the facilitating daemon stays the file-access identity

`session-room.md` § File access turns on "participants never learn which case they are in". Adding a
second *daemon* participant that serves only its own agents' conversations keeps that property: no
participant re-addresses its file RPCs.

### A clone's checkout is a detached worktree under the sessions base, not a branch

It is still a `workspace` session — listable, deletable, provisioned from the same project registry —
but its checkout is **not** cut by `start_workspace_session`'s branch workflow. Three reasons, and
each of them is a failure the branch workflow would produce:

- A mirror has no branch of its own. The sync resets it onto the facilitating session's `HEAD` and
  fills it from that session's WIP tree on the first tick, so a branch cut here is moved off moments
  later — and a *named* one shows up in `git branch` of a repository the operator shares with their
  own work.
- The branch workflow fetches a remote-tracking integration base (`origin/main`) before it can cut
  anything. A mirror needs no remote: everything it will ever hold comes from the session's WIP ref,
  so requiring one refuses a perfectly mirrorable project.
- It lives inside the session directory rather than under the project's `.worktrees/`, so it is
  removed with the session and an operator looking at their project does not find another agent's
  checkout beside their own. `git worktree remove` refuses a checkout carrying modified or untracked
  files — which a mirror of a session with uncommitted work *always* does — so the registration is
  cleared by `git worktree prune` after the directory goes, rather than by forcing a removal.

### The session room publishes its WIP ref on the first tick, not only on change

Pre-existing gap this feature is the first to hit: `publish_wip_ref` ran inside `announce`, which
only runs when the checkout has *moved*. A participant that attached to a **quiet** session therefore
had nothing to restore from and stayed at whatever it happened to check out, indefinitely. The room
now stages and publishes once when it opens. No delta is recorded there — a delta is a diff between
two ticks, and one taken against no predecessor would be the whole worktree, which a client would
apply on top of a checkout that already contains it.

### The owning daemon's agent surface is served on the common room

The PRD reads "B joins as `daemon-{B}` and serves its agent surface there". B does join
`session-{id}` as `daemon-{B}` — that is where its mirror subscribes to `worktree.activity` /
`session.activity` and where its mutating tools proxy back to the facilitating daemon — but the
conversation RPCs the facilitating daemon forwards ride the **common room**, which is the link every
other cross-daemon call already uses and the only one on which "this daemon has gone away" is
already answerable. Serving a second RPC surface on the session-room connection would need
`LiveKitParticipant` to expose its room as a shared handle so one connection could both serve and
subscribe, which it does not; the alternative — a second connection under the same identity — is one
LiveKit disconnects. Nothing observable differs: the main agent addresses the facilitating daemon and
never learns which link carried its prompt.

### `ReportAgentCloneState` is pushed, not polled

Readiness, the checkout's path and every reconcile are facts only the daemon holding the checkout can
state. The facilitating daemon owns the roster and answers every read of it, so it has to be *told*:
a poll would have it deciding an entry is ready from the outside, which is how a prompt gets served
from an empty tree. The report is refused for any clone this daemon did not itself ask that daemon
for, and for one naming a different checkout — the report is what authorizes an entry to start
serving prompts, so accepting an unknown one would let any room participant mark an agent ready.

### No seeded `fastcontext.yaml`

Shipping a seed file would reintroduce the hardcoded default one directory further out. An operator
who wants it writes it; the format is unchanged.

## Implementation Milestones

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit tests
- [x] Proto additions + codegen (`session.agents` topic constant landed with the broadcast)
- [x] `tddy-core`: `AgentId`, `SessionAgentRecord`, `SessionMetadata` field swap
- [x] `tddy-discovery`: delete builtins, `subagent_replaced_tools`, `resolve_replaced_tools`;
      `FastContextBackend` → `SpecializedAgentBackend`
- [x] `tddy-daemon`: `SessionAgentRosterStore` — attach/detach/snapshot/subscribe/persist
- [x] `tddy-daemon`: the four roster RPCs + tonic adapters
- [x] `tddy-daemon`: `ListSubagents` daemon stamping + qualified-id resolution across peers
- [x] `tddy-daemon`: `session.agents` broadcast + owning-daemon room admission handshake
      (`SessionAdmissionService.AdmitOwningDaemon` + `SessionAdmissionRegistry` + re-admit loop +
      revocation on last detach / session delete) — closed 2026-08-18
- [x] `tddy-daemon`: clone provisioning, in-process sync, release on last detach — including
      provisioning on a daemon that has **never seen the project** (AC37, via
      `remote_git.RemoteGitService` over the common room + `tddy-remote-git-repo` transport)
- [x] `tddy-daemon`: conversation routing (local + remote) and the three RPCs
- [x] `tddy-daemon`: allowlist from roster at spawn and resume; teardown of every clone
- [x] `tddy-tools`: roster stream client with reconnect; registry from roster; `tools/list_changed`
- [~] `tddy-tools`: remote conversation dispatch **[ ]** (daemon-side handlers exist; the jail
      cannot reach them, see the sandbox bridge); replaced-tool refusal **[x]**; no `TDDY_SUBAGENT`
      default **[x]**
- [x] `tddy-sandbox`: appendix from roster
- [x] `tddy-sandbox-app`: delete action-author/coder validation
- [x] `tddy-coder`: delete `--fastcontext-*`; roster for coder-hosted sessions
- [x] `tddy-web`: fanned-out picker; Agent roster pane
- [x] `clippy --workspace --all-targets -D warnings` + `fmt --check` clean
- [ ] **Pre-PR review remediation** — in flight, see § Current status

## Current status (2026-08-17)

**Implementation complete; pre-PR review remediation in progress. Not ready to PR.**

### Test evidence — 101 of 102 target tests green, verified by re-running each suite

| Suite | Result |
|---|---|
| `tddy-core` · `session_agent_roster` | 8 / 8 |
| `tddy-discovery` · `no_builtin_agents_acceptance` | 9 / 9 |
| `tddy-discovery` · `subagent_loop_red` (synthesis restored) | 3 / 3 |
| `tddy-discovery` · `agent_def_red` (+2 required-`base_url`) | 7 / 7 |
| `tddy-coder` · `specialized_agent_backend_acceptance` | 6 / 6 |
| `tddy-daemon` · `session_agent_roster_acceptance` | 16 / 16 |
| `tddy-daemon` · `session_agent_replacement_acceptance` | 12 / 12 |
| `tddy-daemon` · `session_agent_remote_acceptance` | **18 / 20** (shared testkit) |
| `tddy-daemon` · `session_agent_conversation_acceptance` (new) | 4 / 4 |
| `tddy-daemon` (lib) | 518 / 0 |
| `tddy-tools` · `session_agent_roster_client_acceptance` | 16 / 16 |
| `tddy-tools` · `subagent_tool_advertisement_acceptance` | 2 / 2 |
| `tddy-tools` (whole package) | 272 / 0 |
| `tddy-web` · `SessionAgentRosterPane` + `CreateSessionAgentPicker` | 13 / 13 + 5 / 5 |

The one genuine failure is `provisions_the_project_on_a_daemon_that_has_never_seen_it` (AC37).
Three other tests in that suite intermittently fail on a Docker port collision
(`failed to bind host port …: address already in use`) and pass on re-run — the pre-existing
LiveKit-testkit flake, and it hits *different* tests each run, which is how it is distinguished
from a real failure.

### Pre-PR review findings — three reviews, 8 CRITICAL — ALL REMEDIATED

Every CRITICAL was independently verified against the code before being actioned, and every fix was
verified by mutation (revert the production change, watch the new test go red) rather than by
reasoning. Four remediation passes landed.

**Security (`tddy-daemon`) — fixed, each with a test**
1. `execute_tool` / `stream_execute_tool` run the hosted-clone branch **before**
   `resolve_exec_tool_worktree`, which is the only auth on that path — an unauthenticated caller
   can `Write` into another host's authoritative worktree using the clone's stored token.
2. `report_agent_clone_state` never reads `session_token`; anyone who has seen a `session.agents`
   broadcast can flip a provisioning clone to `READY`, so the next prompt is served from an empty
   checkout.
3. `session_dir_for` joins a caller-supplied `session_id` with no
   `validate_session_id_segment` — used 28× elsewhere in the same file — so a traversal id
   read-modify-writes another session's `.session.yaml`.
4. The conversation-map lock is held across the whole turn loop, so `subagent_cancel` and
   detach-cancellation cannot proceed until the turn they exist to interrupt has finished.

**Test fidelity — two suites did not exercise production code — fixed**
5. `replaced_tools_for_session` had **zero production callers**; the spawn path uses
   `roster_replacement_pairs`. All 12 replacement tests validated a parallel copy.
   **Fixed:** the parallel copy is deleted and the suite now drives
   `attach RPC → .session.yaml → read_session_metadata().agents → roster_replacement_pairs`, the
   chain `resume_sandboxed_session` runs verbatim. Mutation-verified: gutting
   `roster_replacement_pairs`, and separately stopping `persist_roster` from writing `meta.agents`,
   each turn 6 of the 12 red.
6. `execute_agent_tool` / `agent_tool_args` had no production caller — AC31/AC32, the justification
   for the whole clone design, asserted against a method a remote agent's turn loop never invokes.
   **Fixed:** both deleted; AC31/AC32 now drive a real conversation whose stub model issues a
   `READ`/`WRITE` tool call, asserting where the effect landed. Mutation-verified: inverting the
   read/write split, and separately forcing `CodebaseAccess::Local`, each turn both red — the
   mutation the old tests could not see.
7. The jail-side roster client (connect, first-frame deadline, backoff, give-up) has no test at all,
   while the transport it needs is the unimplemented sandbox bridge. **Not closed** — it is the
   sandbox-bridge blocker recorded in [docs/dev/TODO.md](../TODO.md); the suites' headers should say
   the transport is unimplemented rather than reading as coverage.

**Capability loss (`tddy-tools`) — fixed**
8. `check_tool_available` kept enforcing the withdrawn set while the roster was `Unavailable` — and
   in a jail it is *permanently* unavailable, so a managed session with a `replaces: [Grep, Glob]`
   agent ended up with no search capability at all. **Fixed** with an explicit
   `RosterCurrency::{Seeded, Current, Stale, Unreachable}` state: a roster that *was* current and
   went stale still enforces; one that never received a frame does not. The invariant is stated in
   the module doc — a withdrawal never outlives the reachability of its replacement.

Lower-severity findings actioned in the same pass: a `TDDY_SUBAGENTS_JSON` parse failure silently
disabling all tool withdrawal on version skew (this branch added `api_key` to a
`deny_unknown_fields` struct); the restored synthesis turn discarding usage on error and retaining
its "no more tools" instruction across later prompts; `try_qualified` minting ids `parse` refuses;
a mirror `restore()` that logs a false divergence on every ordinary edit; an orphaned clone on a
failed attach; a misleading teardown error; an unbounded conversation frame; the Agents inspector
tab having no spec.

### Change Validation (@validate-changes) — 2026-08-17

**Last Run**: 2026-08-17
**Status**: ⚠️ Warnings (1 documented gap, not a regression)
**Risk Level**: 🟡 Medium — feature complete, one deliberate AC37 gap, two known flake classes

**Build Validation** (`./dev cargo build --workspace --tests`):

| Package | Status | Notes |
|---|---|---|
| workspace (all 13 affected crates + tests) | ✅ Pass | Built in 20m 15s after a disk-full retry; no errors, no warnings on changed code. The two `fake_lsp` output-filename collisions are pre-existing (`tddy-lsp` vs `tddy-lsp-executor`) and unrelated to this changeset. |

**Test evidence** (re-run by @validate-changes, 2026-08-17):

| Suite | Result |
|---|---|
| `tddy-core` · `session_agent_roster` | 12 / 12 |
| `tddy-discovery` · `no_builtin_agents_acceptance` | 9 / 9 |
| `tddy-discovery` · red suites (`subagent_loop_red`, `agent_def_red`, `specialized_subagent_red`, `subagent_session_red`, `subagent_usage_red`, `subagent_write_tools_red`, `subagent_replaced_tools_acceptance`) | 38 / 38 |
| `tddy-coder` · `specialized_agent_backend_acceptance` | 6 / 6 |
| `tddy-daemon` · `session_agent_roster_acceptance` | 16 / 16 |
| `tddy-daemon` · `session_agent_replacement_acceptance` | 12 / 12 |
| `tddy-daemon` · `session_agent_conversation_acceptance` | 4 / 4 |
| `tddy-daemon` · `session_agent_remote_acceptance` (LiveKit, `#[serial]`) | 17 / 20 — 1 genuine AC37 failure (documented below), 2 Docker port-collision flakes that **pass on re-run** (`gives_each_owning_daemon_its_own_clone`, `clones_into_a_checkout_that_is_neither_the_project_nor_a_worktree_in_use`; both verified green in isolation immediately after) |
| `tddy-tools` · `session_agent_roster_client_acceptance` | 22 / 22 |
| `tddy-tools` · `subagent_tool_advertisement_acceptance` | 2 / 2 |
| `tddy-web` · `SessionAgentRosterPane.cy.tsx` | 14 / 14 |
| `tddy-web` · `CreateSessionAgentPicker.cy.tsx` | 5 / 5 |

**Deterministic total**: 157 / 157 green. The remote suite's only genuine failure is `provisions_the_project_on_a_daemon_that_has_never_seen_it` (AC37), already recorded in [docs/dev/TODO.md](../TODO.md) § `provisions_the_project_on_a_daemon_that_has_never_seen_it` — AC37 is not implemented and the test was deliberately not weakened. The other two remote failures are the pre-existing LiveKit-testkit port-collision flake (it hits *different* tests each run, as the changeset already states), and they pass on re-run.

**Security re-verification** — all four CRITICAL fixes from the pre-PR reviews confirmed in code:

1. `execute_tool` / `stream_execute_tool` (`connection_service.rs:10776-10791`, `10832-10849`) run `authorize_exec_tool_caller` **before** `hosted_clone_for` / `resolve_exec_tool_worktree` — the hosted-clone branch can no longer be reached with no credential.
2. `report_agent_clone_state` (`connection_service.rs:8610`) calls `roster_session_dir(&req.session_token, &req.session_id)?` first — a participant that saw a `session.agents` broadcast can no longer flip a provisioning clone to `READY`.
3. `session_dir_for` (`connection_service.rs:3077-3081`) validates `session_id` with `validate_session_id_segment` before joining — a traversal id can no longer read-modify-write another session's `.session.yaml`. Used at every roster/conversation RPC site that takes a caller-supplied `session_id`.
4. `prompt_agent_conversation` (`connection_service.rs:8449-8470`) takes the routing out of `agent_conversations` under one lock and **drops the guard before** the turn loop (spawned at `:8503`); the turn `select!`s against `closed.notified()`, so `cancel_agent_conversation` and detach-cancellation can interrupt an in-flight turn.

**TODO markers**: 11 `TODO(session-agent-roster)` markers confirmed in tree (5 in `connection_service.rs`, 1 in `session_agent_clone.rs`, 3 in `tddy-tools/src/session_agents.rs`, 1 in `tddy-tools/src/server.rs`, 1 in `tddy-sandbox-runner/src/runner.rs`) — matches the changeset's "Deferred work marker in the tree" table.

**Production/test separation**: no `cfg!(test)` runtime branches in any new file (`session_agent.rs`, `session_agent_roster.rs`, `session_agent_clone.rs`, `tddy-tools/src/session_agents.rs`, `tddy-service/src/session_agents.rs`); `#[cfg(test)]` modules are test-only at file bottom (standard Rust pattern). No FIXME markers in new production code.

**Risk Assessment**:

| Category | Risk | Notes |
|---|---|---|
| Build validation | 🟢 Low | Workspace builds clean; only pre-existing `fake_lsp` collision warnings. |
| Test infrastructure | 🟢 Low | No mocks in production; fluent-tests style followed; in-memory backends for Cypress. |
| Production code | 🟡 Medium | Two untested-by-design remediation paths (orphaned-clone unwind, detach-cancel forwarding) shipped deliberately — both need fault-injection seams that would be test-only production code. Documented in "Technical Debt". |
| Security | 🟢 Low | All 4 CRITICAL fixes verified in code; `validate_session_id_segment` applied at every caller-supplied `session_id` site. Residual: `HostedClone.session_token` is the caller's own token held for the clone's life (TODO at `session_agent_clone.rs:266`). |
| Code quality | 🟢 Low | New modules are well-decomposed; long functions (`run_clone_mirror`, `prompt_agent_conversation`) carry explanatory comments justifying their length. |
| Changeset alignment | 🟢 Low | Changeset "Implementation Milestones" and "Current status" match the code; deviations are recorded rather than silently absorbed. |

**Open items blocking PR** (already tracked, not new findings):

- AC37 — `provisions_the_project_on_a_daemon_that_has_never_seen_it`; needs `remote_git.RemoteGitService` on a peer-reachable route. TODO at `connection_service.rs:3278`.
- The sandbox-IPC bridge for `StreamSessionAgents` / conversation RPCs — TODO at `tddy-sandbox-runner/src/runner.rs:44`; a managed-codebase session's specialized agents are unusable until it lands (the correct refusal is in place).
- The room-admission handshake (PRD deviation, see § Deviations).
- `HostedClone.session_token` lifetime (TODO at `session_agent_clone.rs:266`).

### Deviations from the PRD, recorded rather than silently absorbed

- ~~**No room-admission handshake.**~~ **Closed 2026-08-18.** The handshake is now built: the
  facilitating daemon mints a scoped, short-TTL (5 min) admission token in
  `provision_agent_clone`, records the owning daemon in a `SessionAdmissionRegistry`, and
  forwards the token inside `StartSessionRequest.agent_clone` (`AgentClonePlacement.first_admission_*`).
  The owning daemon joins `session-{id}` with that token (no self-mint), and `run_clone_mirror`
  runs a re-admit loop: on room disconnect it calls `SessionAdmissionService.AdmitOwningDaemon`
  over the common room for a fresh token and rejoins; a `FAILED_PRECONDITION` re-admit is the
  revocation signal (last agent detached / session deleted) and the mirror stops. Revocation is
  wired in `tear_down_agent_clone` (last detach) and `delete_session` (`revoke_all_for_session`).
  The PRD § "What attach does" step 3 has been rewritten to describe what was built.
- **Conversation RPCs ride the common room**, not the session room (§ The owning daemon's agent
  surface is served on the common room).
- **`ReportAgentCloneState` + `StartSessionRequest.agent_clone`** are new wire surface the PRD did
  not specify (§ `ReportAgentCloneState` is pushed, not polled). The PRD now documents them.
- ~~**AC15 is unmet on both paths.**~~ **Closed during remediation.** Local conversations are now
  keyed by (session, agent) and signalled through an `Arc<Notify>`; remote ones get a
  `CancelAgentConversation` forwarded to the owning daemon. A cancel now interrupts a turn that is
  still in flight — which the PRD asserted and no code implemented.

Deferred work is recorded in [docs/dev/TODO.md](../TODO.md) § *Session agent roster — the sandbox
bridge, and other deliberate gaps* and § *`provisions_the_project_on_a_daemon_that_has_never_seen_it`*.

## Testing Plan

### Testing Strategy

**Primary level: acceptance per package**, because each layer is independently wrong-able and the
expensive layer (a real second daemon in a real room) is needed for only a slice of the behaviour.

- The **roster store** is pure state with persistence — testable with no RPC and no room.
- The **roster RPCs** are testable against a single daemon with an in-memory transport, which
  already covers attach, detach, idempotency, revisioning, refusals and streaming.
- The **live registry** is testable in `tddy-tools` against a stub roster stream — no daemon.
- **Remote agents and clones** genuinely need two daemons and a room; that is one production suite,
  not a level every test pays for.
- The **web** is Cypress component tests with `mountWithRpc` + `anInMemoryRpcBackend`, per house
  rule — never `cy.intercept`.

**Deliberately not unit tests**: the qualified-id round trip and the replaced-union computation are
unit-level and are tested as such; everything about *who answered* is acceptance-level, because the
bug this feature exists to prevent is an answer from the wrong host.

### Option 1: roster store and RPCs (primary)

**Test level**: Integration
**Location**: `packages/tddy-daemon/tests/session_agent_roster_acceptance.rs` (new)

**Scope**: AC1-12 — attach, idempotent re-attach, unresolvable id, unqualified id, ten agents,
detach ordering, unknown detach, `List`/`Stream` agreement, immediate first frame, no frame for a
no-op, persistence across a store rebuild, legacy `.session.yaml`, auth refused before any peer
contact.

**Reliability**: deterministic — a temp sessions base, no LiveKit, no peer.

### Option 2: live registry in tddy-tools (primary)

**Test level**: Integration
**Location**: `packages/tddy-tools/tests/session_agent_roster_client_acceptance.rs` (new)

**Scope**: AC13-18, 20-21 — seed then first frame replaces it; an added agent callable without a
restart; a removed agent refused and its conversation cancelled; one `tools/list_changed` per
frame; a stream that cannot be maintained refuses rather than serving the seed; no `TDDY_SUBAGENT`
default; a replaced tool refused naming the agent; detach restoring it in-process.

**Reliability**: deterministic — a stub roster server over an in-process duplex, frames pushed
explicitly.

### Option 3: tool replacement and spawn wiring (primary)

**Test level**: Integration
**Location**: `packages/tddy-daemon/tests/session_agent_replacement_acceptance.rs` (new)

**Scope**: AC19, 22-25 — the union across two agents; two agents both replacing `Shell` accepted;
an agent replacing a tool it does not bind accepted; a `replaces`-carrying attach to a non-managed
session refused; allowlist/disallowlist computed from the persisted roster at spawn and resume.

### Option 4: remote agents and clones (secondary)

**Test level**: Production
**Location**: `packages/tddy-daemon/tests/session_agent_remote_acceptance.rs` (new)

**Scope**: AC26-43 — two daemons, a real room, a real git project: remote resolution, room
admission, indistinguishable prompt results, clone sharing and separation, local reads, proxied
writes, provisioning refusal, unreachable peers, in-flight work reaching the clone, commit
following, reconcile, teardown on last detach, teardown on session delete.

**Reliability**: `#[serial]`, multi-thread runtime, `tddy_livekit_testkit::LiveKitTestkit`, matching
`multi_host_acceptance.rs`.

### Option 5: de-hardcoding (primary)

**Test level**: Unit + Integration
**Location**: `packages/tddy-discovery/tests/no_builtin_agents_acceptance.rs` (new),
`packages/tddy-coder/tests/specialized_agent_backend_acceptance.rs` (replaces
`fastcontext_backend_acceptance.rs`)

**Scope**: AC44-47 — an empty agents dir resolves to nothing; a backend's model/base URL/turn budget
come only from the def; a session with an empty roster starts with the full native tool set. AC45
("no identifier contains `fastcontext`") is a repo-level assertion, run as a test over the source
tree rather than trusted to review.

### Option 6: web (primary)

**Test level**: Cypress component
**Location**: `packages/tddy-web/cypress/component/SessionAgentRosterPane.cy.tsx` (new),
`packages/tddy-web/cypress/component/CreateSessionAgentPicker.cy.tsx` (new)

**Scope**: AC48-53 — the picker lists every daemon's agents labelled by host and sends qualified
ids; one daemon's failure costs one row; the pane updates on a pushed roster frame; the add flow
states what the main agent loses; the last-remote-agent detach confirms naming the host; four
distinct states.

**Reliability**: deterministic — `mountWithRpc` + `anInMemoryRpcBackend`, roster frames pushed by
the fake backend.

### Coverage Requirements

- [ ] **Happy path**: attach local, attach remote, prompt both, detach both
- [ ] **Error scenarios**: unresolvable id, unqualified id, unknown detach, unreachable owner,
      clone provisioning failure, prompt before clone ready, roster stream lost, replaced tool
      called, non-managed attach with `replaces`
- [ ] **Edge cases**: idempotent re-attach, ten agents, two agents on one daemon, two daemons,
      two agents both replacing `Shell`, an agent replacing a tool it does not bind, empty roster
- [ ] **API boundaries**: `ListSubagents` daemon stamping, `AgentId` round trip, snapshot-first
      streaming, auth before side effects
- [ ] **Regression**: legacy `.session.yaml` loads; `worktree.activity` and `session.activity`
      unchanged; co-located sessions with no roster behave exactly as before

## Acceptance Tests

Written and confirmed failing. Names below are the functions as they exist on disk.

### tddy-core — `tests/session_agent_roster.rs`
- [ ] **Unit**: `formats_an_agent_id_that_parses_back_to_the_same_pair` (AC4)
- [ ] **Unit**: `refuses_a_name_that_would_make_its_own_id_ambiguous` (AC4)
- [ ] **Unit**: `refuses_an_id_with_no_daemon_part` (AC4)
- [ ] **Unit**: `refuses_an_id_whose_daemon_part_is_empty` (AC4)
- [ ] **Unit**: `round_trips_a_roster_through_the_session_file` (AC10)
- [ ] **Unit**: `omits_the_roster_from_the_session_file_when_it_is_empty` (AC10)
- [ ] **Unit**: `reads_a_session_file_that_predates_the_roster` (AC11)
- [ ] **Unit**: `never_writes_the_superseded_agent_names_back_to_the_session_file` (AC11)

### tddy-daemon — `tests/session_agent_roster_acceptance.rs`
- [ ] **Integration**: `attaching_an_agent_adds_it_to_the_roster_and_advances_the_revision` (AC1)
- [ ] **Integration**: `attaching_the_same_agent_twice_leaves_the_revision_where_it_was` (AC2)
- [ ] **Integration**: `refuses_an_agent_no_daemon_can_resolve_and_leaves_the_roster_alone` (AC3)
- [ ] **Integration**: `refuses_an_agent_id_that_does_not_name_its_daemon` (AC4)
- [ ] **Integration**: `attaches_ten_agents_and_addresses_every_one_of_them` (AC5)
- [ ] **Integration**: `detaching_one_agent_leaves_the_others_in_the_order_they_were_attached` (AC6)
- [ ] **Integration**: `refuses_to_detach_an_agent_that_was_never_attached` (AC7)
- [ ] **Integration**: `reports_the_same_roster_whether_it_is_listed_or_streamed` (AC8)
- [ ] **Integration**: `hands_a_new_subscriber_the_current_roster_before_anything_changes` (AC9)
- [ ] **Integration**: `publishes_no_frame_for_an_attach_that_changed_nothing` (AC9)
- [ ] **Integration**: `restores_the_roster_and_its_revision_after_the_daemon_restarts` (AC10)
- [ ] **Integration**: `reads_a_session_written_before_rosters_existed_as_having_no_agents` (AC11)
- [ ] **Integration**: `refuses_an_unauthenticated_roster_call_before_contacting_any_peer` (AC12)
- [ ] **Integration**: `refuses_an_unauthenticated_read_of_the_roster` (AC12)

### tddy-daemon — `tests/session_agent_replacement_acceptance.rs`
- [ ] **Integration**: `withdraws_every_tool_any_attached_agent_replaces` (AC19)
- [ ] **Integration**: `keeps_a_tool_withdrawn_while_any_agent_still_replaces_it` (AC19)
- [ ] **Integration**: `restores_a_tool_once_the_last_agent_replacing_it_is_detached` (AC21)
- [ ] **Integration**: `accepts_two_agents_that_both_replace_the_shell_tool` (AC22)
- [ ] **Unit**: `replacing_shell_no_longer_grants_the_session_action_tools` (AC22)
- [ ] **Unit**: `keeps_the_native_bash_family_unreachable_when_shell_is_replaced` (AC22)
- [ ] **Unit**: `keeps_the_native_edit_family_unreachable_when_write_is_replaced` (AC22)
- [ ] **Integration**: `accepts_an_agent_that_replaces_a_tool_it_cannot_serve_itself` (AC23)
- [ ] **Integration**: `refuses_a_replacing_agent_on_a_session_whose_tools_it_cannot_reach` (AC24)
- [ ] **Integration**: `accepts_a_non_replacing_agent_on_a_non_managed_session` (AC24)
- [ ] **Integration**: `launches_a_resumed_session_without_the_tools_its_roster_replaced` (AC25)
- [ ] **Integration**: `launches_a_session_with_no_agents_holding_every_tool` (AC25/AC47)

### tddy-daemon — `tests/session_agent_remote_acceptance.rs` (LiveKit, `#[serial]`)
- [ ] **Production**: `resolves_a_remote_agent_from_the_daemon_that_defines_it` (AC26)
- [ ] **Production**: `brings_the_owning_daemon_into_the_session_room` (AC27)
- [ ] **Production**: `keeps_the_facilitating_daemon_as_the_identity_file_reads_are_addressed_to` (AC27)
- [ ] **Production**: `answers_a_prompt_to_a_remote_agent_the_way_a_local_one_answers` (AC28)
- [ ] **Production**: `serves_two_agents_of_one_daemon_from_a_single_clone` (AC29)
- [ ] **Production**: `gives_each_owning_daemon_its_own_clone` (AC30)
- [ ] **Production**: `reads_a_remote_agents_files_from_its_own_clone` (AC31)
- [ ] **Production**: `lands_a_remote_agents_write_in_the_authoritative_worktree` (AC32)
- [ ] **Production**: `refuses_a_prompt_while_the_clone_is_still_being_built` (AC33)
- [ ] **Production**: `leaves_nothing_behind_when_the_owning_daemon_cannot_be_reached` (AC34)
- [ ] **Production**: `fails_only_the_agents_of_a_daemon_that_goes_away` (AC35)
- [ ] **Production**: `clones_into_a_checkout_that_is_neither_the_project_nor_a_worktree_in_use` (AC36)
- [ ] **Production**: `provisions_the_project_on_a_daemon_that_has_never_seen_it` (AC37)
- [ ] **Production**: `shows_a_remote_agent_work_the_main_agent_has_not_committed` (AC38)
- [ ] **Production**: `moves_the_clone_to_the_commit_the_session_made` (AC39)
- [ ] **Production**: `restores_a_clone_that_diverged_and_says_so` (AC40)
- [ ] **Production**: `removes_the_clone_when_the_last_agent_on_that_host_is_detached` (AC41)
- [ ] **Production**: `treats_a_clone_the_peer_already_removed_as_removed` (AC42)
- [ ] **Production**: `removes_every_clone_when_the_session_is_deleted` (AC43)

### tddy-tools — `tests/session_agent_roster_client_acceptance.rs`
- [ ] **Integration**: `replaces_the_seeded_registry_with_the_first_roster_it_receives` (AC13)
- [ ] **Integration**: `ignores_a_roster_frame_older_than_the_one_already_applied` (AC13)
- [ ] **Integration**: `opens_a_conversation_with_an_agent_attached_after_it_started` (AC14)
- [ ] **Integration**: `refuses_an_agent_that_was_detached_and_names_the_id` (AC15)
- [ ] **Integration**: `cancels_a_conversation_whose_agent_was_detached_underneath_it` (AC15)
- [ ] **Integration**: `leaves_a_conversation_open_when_a_different_agent_is_detached` (AC15)
- [ ] **Integration**: `announces_exactly_one_tool_list_change_per_roster_revision` (AC16)
- [ ] **Integration**: `announces_nothing_when_a_reconnect_redelivers_the_revision_already_applied` (AC16)
- [ ] **Integration**: `refuses_subagent_calls_when_it_cannot_keep_the_roster_current` (AC17)
- [ ] **Integration**: `refuses_rather_than_serving_the_spawn_seed_forever` (AC17)
- [ ] **Integration**: `serves_again_once_the_roster_stream_recovers` (AC17)
- [ ] **Integration**: `refuses_a_conversation_that_names_no_agent_and_lists_the_ones_it_has` (AC18)
- [ ] **Integration**: `refuses_a_conversation_when_no_agent_is_attached_at_all` (AC18)
- [ ] **Integration**: `refuses_a_replaced_tool_and_names_the_agent_that_serves_it` (AC20)
- [ ] **Integration**: `allows_a_tool_no_attached_agent_replaces` (AC20)
- [ ] **Integration**: `allows_a_replaced_tool_again_once_its_agent_is_detached` (AC20/AC21)

### tddy-discovery — `tests/no_builtin_agents_acceptance.rs`
- [ ] **Integration**: `resolves_no_agents_at_all_from_an_empty_directory` (AC44)
- [ ] **Integration**: `resolves_no_agents_from_a_directory_that_does_not_exist` (AC44)
- [ ] **Integration**: `resolves_exactly_the_agents_a_directory_defines` (AC44)
- [ ] **Integration**: `resolving_adds_nothing_to_what_was_loaded` (AC44)
- [ ] **Integration**: `no_production_source_names_the_agent_that_used_to_be_builtin` (AC45)
- [ ] **Integration**: `no_production_source_carries_the_builtin_agents_model_id` (AC45)
- [x] **Integration**: `treats_a_def_named_after_the_old_builtin_as_an_ordinary_def` — regression guard, green today
- [x] **Unit**: `computes_a_replaced_set_only_from_the_defs_it_is_given` — regression guard, green today
- [x] **Unit**: `withdraws_nothing_when_no_agent_is_attached` — regression guard, green today

### tddy-coder — `tests/specialized_agent_backend_acceptance.rs`
Replaces `tests/fastcontext_backend_acceptance.rs`, which asserts the opposite and is deleted in green.
- [ ] **Integration**: `no_longer_accepts_a_flag_that_could_override_a_defs_endpoint` (AC46)
- [ ] **Integration**: `no_longer_accepts_a_flag_that_could_override_a_defs_model` (AC46)
- [ ] **Integration**: `no_longer_accepts_a_flag_that_could_override_a_defs_turn_budget` (AC46)
- [ ] **Integration**: `still_accepts_an_operator_defined_agent_name` (AC46)
- [ ] **Integration**: `the_help_text_names_no_hardcoded_agent` (AC45)
- [ ] **Integration**: `the_shipped_dev_daemon_config_lists_no_hardcoded_agent` (AC45)

### tddy-web — `cypress/component/SessionAgentRosterPane.cy.tsx`
- [ ] **Component**: `shows an agent attached from somewhere else without being asked to refresh` (AC50)
- [ ] **Component**: `drops an agent detached from somewhere else` (AC50)
- [ ] **Component**: `names the host each attached agent belongs to` (AC50)
- [ ] **Component**: `shows the tools an attached agent takes away from the main agent` (AC50)
- [ ] **Component**: `shows a remote agent's clone as provisioning until it is ready` (AC50)
- [ ] **Component**: `says which tools the main agent loses before the operator confirms` (AC51)
- [ ] **Component**: `attaches the agent under its qualified id when the operator confirms` (AC51)
- [ ] **Component**: `asks before a detach that deletes a checkout on another host` (AC52)
- [ ] **Component**: `detaches a local agent without asking, because no checkout is destroyed` (AC52)
- [ ] **Component**: `shows a disconnected host as disconnected rather than as an empty roster` (AC53)
- [ ] **Component**: `shows loading while the first roster frame has not arrived` (AC53)
- [ ] **Component**: `shows a failed read as an error naming the failure, not as an empty roster` (AC53)
- [ ] **Component**: `shows a genuinely empty roster as empty` (AC53)

### tddy-web — `cypress/component/CreateSessionAgentPicker.cy.tsx`
- [ ] **Component**: `lists agents from every connected host labelled by the host that offers them` (AC48)
- [ ] **Component**: `sends the qualified id of every agent the operator picked` (AC48)
- [ ] **Component**: `sends no agents when the managed-codebase section is left closed` (AC48)
- [ ] **Component**: `costs one row when a host cannot answer rather than the whole picker` (AC49)
- [ ] **Component**: `still starts a session when one host could not be listed` (AC49)

### Test support added
| File | Purpose |
|------|---------|
| `packages/tddy-web/cypress/support/rpc/sessionAgentRosterBackend.ts` | Pushable roster stream, per-host `ListSubagents`, attach/detach recording |
| `packages/tddy-web/cypress/support/pages/sessionAgentRosterPage.ts` | Page object for the roster pane and picker |
| `packages/tddy-web/cypress/support/testIds.ts` | `agentRoster*` ids + `createSessionAgent*` helpers |

## Technical Debt & Production Readiness

Carried deliberately, each recorded in `docs/dev/TODO.md` under this changeset:

- **A remote agent's `SHELL` runs on the facilitating daemon.** The host whose toolchain motivated
  attaching the agent is not the host its commands run on. Fixing it needs write-back.
- **A clone is a full checkout.** Ten remote agents across ten daemons means ten checkouts. No
  quota, no eviction, no shared object store.
- ~~**There is no room-admission handshake at all.**~~ **Closed 2026-08-18.** The handshake is
  built: `SessionAdmissionRegistry` + `SessionAdmissionService.AdmitOwningDaemon` + the
  re-admit loop in `run_clone_mirror` + revocation on last detach and session delete. See
  § Deviations for the close-out.
- **The roster stream is the first long-lived server stream over `SandboxIpc`.** Reconnect and
  backoff are implemented here rather than in the transport.
- **`specialized_agents` in existing `.session.yaml` files is dropped, not migrated.** Sessions
  resumed across the upgrade lose their agents and must re-attach.
- **An owning daemon that does not already hold the project cannot provision it (AC37).**
  `ensure_project_available_locally` resolves an unknown project through that daemon's *own* peer
  fan-out and then `git clone <git_url>` — which fails for the two cases that matter: a peer that is
  in the facilitating daemon's room but keeps no room of its own has nobody to ask, and a project
  whose `git_url` names a forge the peer cannot reach has nothing to clone. The facilitating daemon
  already is the authority on that project and already serves `remote_git.RemoteGitService`, so the
  checkout should be fetched from it (`git clone {facilitating_instance_id}:{project_id}` with
  `GIT_SSH_COMMAND=tddy-remote-git-repo`, the transport `tddy-session-sync` already uses). That
  needs the facilitating daemon to serve that service on a route the peer can reach, which is its own
  piece of work. Marked `TODO(session-agent-roster)` at
  `connection_service.rs::provision_agent_clone`.
- **A clone follows committed history only where it can already reach the objects.** The mirror
  restores by fetching `refs/tddy/session/{id}/wip` from the project repository *it* resolved
  locally, which carries the ref only when the two daemons share that repository. Across hosts the
  same `remote_git` transport above is what makes the ref fetchable, so the two are one piece of
  work. Uncommitted edits reported as tool calls still arrive over `StreamAgentActivityDelta`
  regardless.
- **A clone holds the caller's own session token for its whole life.** Marked
  `TODO(session-agent-roster)` on `HostedClone::session_token`: the fix is the one
  `split_session::RoomPollTokenMinter` already applies — mint a fresh short-lived token per call
  under the *verified* caller instead of holding one.
- **The in-jail tool socket still carries only `ExecuteTool`.** `tddy-tools` therefore cannot open
  `StreamSessionAgents` in a sandboxed session and refuses every `subagent_*` call, which is the
  correct refusal and leaves a managed session's specialized agents unusable. The gap is a change to
  the sandbox session channel's contract — it pairs requests and responses positionally, one at a
  time, so a lifetime-long roster stream would occupy it forever — and is spelled out in full at
  `tddy-sandbox-runner/src/runner.rs::ToolExecService`.
  The *capability-loss* half of this is now closed: `RosterCurrency` distinguishes a roster that
  never received a frame (`Unreachable`, does not enforce withdrawal) from one that was current and
  went stale (`Stale`, still enforces), so a jailed session no longer loses tools whose replacement
  it cannot reach.
- ~~**Detaching a *local* agent does not cancel its open conversations.**~~ **Closed during pre-PR
  remediation.** Local conversations are keyed by (session, agent) and signalled through an
  `Arc<Notify>` that a turn `select!`s against; remote ones get a `CancelAgentConversation`
  forwarded to the owning daemon. A cancel now interrupts a turn still in flight.
  ⚠️ Two of those fixes shipped **untested**, and deliberately so: the orphaned-clone unwind needs
  fault injection mid-attach, and detach-cancel forwarding needs a seam for "is conversation X still
  open on peer B". Both seams would be test-only production code.
- ~~**The `Shell` → session-action coupling survives one crate over.**~~ **Closed 2026-08-17.**
  It was worse than the review found: as well as `PermissionServer::new` merging the action router
  when a def replaced `Shell`, `request_action` picked its *author* the same way — so the role lived
  in the call path, not only in advertisement. Both are gone.
  - The action tools are now merged whenever the session has a **host tool transport**. That is a
    real dependency rather than a proxy for a role: all three are pure host round-trips
    (`EstablishAction`/`ListActions`/`InvokeAction`) because the session directory exists only on
    the host, and it is the same fact that already gates the exec catalog. "Always advertise" was
    rejected — it would offer three tools that provably cannot work in a transport-less session.
  - `request_action` now takes an explicit **`agent`** argument resolved against the live roster,
    with the same ids and refusals `subagent_new_session` uses and no default, for the roster's
    documented reason: a default would make the choice depend on attach order.
  - `shell_replacing_author` is deleted; no `replaces`-keyed decision remains in
    `packages/tddy-tools/src`. Guarded by
    `replacing_shell_withdraws_shell_and_changes_nothing_else`, which asserts exact set equality
    between two defs differing only in `replaces`, so any re-added grant fails it. Mutation-verified:
    re-adding the gate turned 6 of 8 tests red.
  - `docs/ft/coder/no-bash-mode.md` carries a supersession banner and its four stale passages are
    corrected; the no-bash *workflow* is unchanged and still supported.
- **An assistant can no longer be created under the name of a `<tddyhome>/agents/*.yaml` def.** The
  old guard was `builtin_agent_defs()`-based and was removed correctly — the PRD explicitly wants an
  assistant named `fastcontext` to be creatable — but the collision that survived it (a registry
  assistant silently shadowing an operator's YAML def, since `resolvable_agent_defs` lets the
  registry win a name tie) went unguarded, and the two tests deleted with the old guard
  (`a_builtin_agent_def_name_is_refused`, `refuses_an_assistant_named_after_a_builtin_agent`) had no
  replacement. `ModelRegistryStore::reject_taken_name` now checks the `<tddyhome>/agents` directory
  as a fourth reserved-name source, refusing with `InvalidName` naming the def's file;
  `ModelRegistryStore::open` takes that directory as a required argument so no construction site can
  leave the guard off. Covered by `model_registry_reserved_names_unit.rs`
  (`a_name_an_agents_directory_def_already_answers_to_is_refused`,
  `a_name_no_agents_directory_def_answers_to_is_accepted`,
  `a_name_claimed_only_by_a_def_file_that_does_not_load_is_accepted`) and
  `registry_assistant_as_agent_acceptance.rs`
  (`an_assistant_cannot_be_created_under_the_name_of_an_agents_directory_def`).
  **Still open the other way round:** a YAML def written *after* an assistant of that name exists
  shadows it silently, since a create-time check cannot see the future.

## Deferred work markers in the tree

Eleven `TODO(session-agent-roster)` markers, each naming what is missing rather than that something
is missing:

| File | What it defers |
|---|---|
| `tddy-daemon/src/connection_service.rs` ×5 | AC37 peer provisioning; a session-scoped tool token; the runner re-deriving `--allowedTools` from the seed; binding a clone-state report to the verified reporting participant; seeding a remote agent at spawn |
| `tddy-daemon/src/session_agent_clone.rs` | `HostedClone` holding the caller's token for the clone's life |
| `tddy-tools/src/session_agents.rs` ×3 | the jail not being told its facilitating daemon's id; the sandbox socket carrying only `ExecuteTool`; no `StreamSessionAgents` client on the HTTP transport |
| `tddy-tools/src/server.rs` | routing the conversation RPCs through the facilitating daemon |
| `tddy-sandbox-runner/src/runner.rs` | carrying the roster and conversation RPCs over the session channel |

## References

- PRD: [docs/ft/daemon/session-agent-roster.md](../../ft/daemon/session-agent-roster.md)
- [Testing practices](../guides/testing.md)
- [Fluent tests](../../../.claude/skills/fluent-tests/references/generic-guidelines.md)
