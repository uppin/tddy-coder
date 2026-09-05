# Session agent roster — attach any number of agents, from any daemon

**Status:** 📝 Planned
**Product area:** Daemon (spans `tddy-service`, `tddy-core`, `tddy-daemon`, `tddy-discovery`,
`tddy-tools`, `tddy-sandbox`, `tddy-sandbox-app`, `tddy-coder`, `tddy-web`)
**Date:** 2026-08-16

## Summary

Today a session's specialized agents are a **fixed list of names, frozen at spawn**: the web sends
`StartSessionRequest.specialized_agents[]`, the facilitating daemon resolves each name against its
*own* def sources, serializes the resolved defs into `TDDY_SUBAGENTS_JSON`, and the in-jail
`tddy-tools --mcp` builds a `SubagentRegistry` from that env var once and never again. An agent
that lives on another daemon cannot be used at all, and one hardcoded agent — `fastcontext` — is
compiled into the binary as a builtin with its own special-cased `replaces` behaviour.

This changes the unit of composition from *a name in a start request* to **a roster attached to a
live session**:

- **Unlimited agents.** The roster is a list with no fixed arity, mutated while the session runs by
  `AttachSessionAgent` / `DetachSessionAgent`, and streamed to every consumer as a revisioned
  snapshot. `TDDY_SUBAGENTS_JSON` becomes a *seed*, not the source of truth.
- **Agents from any daemon.** An agent is addressed as **`name@daemon_instance_id`**. Attaching one
  owned by daemon B makes B **join the session's room** (`session-{session_id}`) and mediate every
  agent it owns there. The main agent never learns which daemon answered.
- **Every remote daemon gets its own clone.** Attaching B's first agent creates a `workspace`
  session on B holding an **independent checkout** of the session's branch — never the project
  directory, never A's worktree — kept current from the session room by the
  [session worktree sync](session-worktree-sync.md) algorithm running in-process on B. B's agents
  read from that clone; their **mutations proxy back** to the facilitating daemon's authoritative
  worktree.
- **Tool binding is purely declarative.** A def's `replaces` list is withdrawn from the main agent
  and nothing else happens. The hardcoded per-tool roles — `Shell` making a def the session's
  "action author", `Write`/`StrReplace`/`Delete` making it the session's "coder", the
  at-most-one-Shell-replacer rule, the "must bind the matching internal tool" validation — are
  **deleted**.
- **No hardcoded agents remain.** `builtin_fastcontext_def()`, `builtin_agent_defs()`,
  `subagent_replaced_tools`'s `"fastcontext"` arm, `FastContextBackend`'s name and the `--agent`
  clap allowlist all go. Every agent comes from a `<tddyhome>/agents/*.yaml` def or a
  [Models & Agents](../web/models-and-agents.md) registry assistant, on some daemon.

```
        daemon A (facilitating)                        daemon B
┌──────────────────────────────────────┐      ┌────────────────────────────────┐
│ claude (in jail)                     │      │ tddy-daemon B                  │
│   Grep/Glob withdrawn                │      │  ┌──────────────────────────┐  │
│   subagent_prompt { "explorer@B" }   │      │  │ workspace session        │  │
│      ↓ MCP (stdio)                   │      │  │  independent clone,      │  │
│ tddy-tools --mcp                     │      │  │  synced from the room    │  │
│   roster @rev N  ◄──StreamSessionAgents─┐   │  └──────────────────────────┘  │
│      ↓ tddy-rpc                      │  │   │   explorer  loop runs here     │
└──────────┬───────────────────────────┘  │   └───────▲────────────────┬───────┘
           │                              │           │                │
     ┌─────▼──────────────────────────────┴───────────┴──────┐         │ mutating
     │ tddy-daemon A — hosts room session-{id}               │◄────────┘ tools
     │   roster owner · routes conversations · authoritative │  StreamExecuteTool
     │   worktree · publishes refs/tddy/session/{id}/wip     │
     └───────────────────────────────────────────────────────┘
```

## User Story

As a developer, I want to attach as many specialized agents to a running session as the work needs
— including agents that live on a workstation with the GPU, the model and the build cache that my
laptop does not have — so that the main agent delegates search, review or linting to whichever
machine can actually do it, against that machine's own checkout, without my having to restart the
session or hardcode anything into the daemon.

## Terminology

Three existing terms are already spoken for and are **not** what this document means:

| Existing term | What it is | Not this |
|---|---|---|
| **Agent** (`ListAgents`) | A *coding backend*: `claude`, `cursor`, `codex-acp` | — |
| **Subagent sessions** | Peer *sessions* spawned under an orchestrator, linked by `orchestrator_session_id` | Not roster entries. They share the Agents tab's tree with roster agents, and the same `SessionAgentStatus` badge, but a daemon runs no loop for them |
| **Assistant** (`models.proto`) | A registry row: model + prompt + tools, projected to a `SpecializedAgentDef` | One *source* of a roster agent, not the roster entry itself |

This document's terms:

| Term | Meaning |
|---|---|
| **Roster** | The ordered set of agents attached to one session. Revisioned, persisted, streamed. |
| **Roster entry** | One attachment: a qualified `agent_id`, the def it resolved to, its owning daemon, and the clone serving it. |
| **Owning daemon** | The daemon whose def sources resolved the agent and whose process runs its turn loop. |
| **Facilitating daemon** | Unchanged from [session-room.md](session-room.md): the daemon running the session's *main* agent, hosting `session-{id}`, holding the authoritative worktree. |

## The roster

### Identity is qualified, always

An agent is `name@daemon_instance_id` — `explorer@ws-01`. Never a bare name.

Bare names cannot work once two daemons contribute defs: `ListSubagents` fanned out across a
common room routinely returns two `explorer` rows, and a roster storing the bare name cannot say
which one the operator picked. Qualifying at attach time also makes the entry **self-routing** — the
daemon to forward a prompt to is read off the id rather than looked up again, so a roster restored
from `.session.yaml` after a restart routes correctly without re-resolving anything.

`name` must not contain `@`; a def whose name does is refused at resolution rather than producing an
id that parses back wrong. `daemon_instance_id` is already constrained to the identity that appears
in `daemon-{instance_id}` participant names.

The qualified id is what the **main agent types**: `subagent_new_session { agent: "explorer@ws-01" }`.
It appears verbatim in the sandbox context appendix, so the agent is told the exact string to use.

### Revision, not diff

Every roster read is a **whole snapshot** carrying a monotonic `rev`:

```proto
message SessionAgentRoster {
  string session_id = 1;
  uint64 rev        = 2;   // bumped on every attach and every detach
  repeated SessionAgentEntry agents = 3;
}
```

A consumer rebuilds its registry from a snapshot. It never applies a delta, so a missed frame costs
staleness until the next one rather than a registry that silently disagrees with the daemon's —
which is the failure mode that matters here, because a disagreeing registry answers
`subagent_new_session` for an agent that was detached, and refuses one that was attached.

`rev` starts at 1 for a session started with a non-empty `specialized_agents` seed, and at 0 for one
started with none.

### An entry

```proto
message SessionAgentEntry {
  string agent_id            = 1;  // "explorer@ws-01" — the id the main agent addresses
  string name                = 2;  // "explorer"
  string daemon_instance_id  = 3;  // "ws-01"
  string label               = 4;  // display only; from the def
  string model               = 5;  // display only; from the def
  // Exec-catalog tools this agent takes over from the main agent. Union'd across the roster.
  repeated string replaces   = 6;
  // Exec-catalog tools the agent's own loop may call.
  repeated string tools      = 7;
  // The workspace session on `daemon_instance_id` holding this daemon's clone. Empty when the
  // owning daemon is the facilitating daemon — a local agent works the real worktree.
  string codebase_session_id = 8;
  // The state of the clone serving this entry. `LOCAL` for a local agent (no clone); provisioning
  // before the owning daemon reports `READY`; `READY` once it has; `ERROR` when provisioning or
  // the mirror failed. An agent is attachable before its clone finishes provisioning; a prompt
  // sent meanwhile is refused naming the state, never queued and never served from an empty
  // checkout.
  AgentCloneState clone_state = 9;
  // A human-readable message when `clone_state == ERROR`; empty otherwise.
  string clone_error         = 10;
  // What the agent is doing right now (§ What an agent is doing). UNSPECIFIED is "this daemon has
  // nothing to say", never "idle".
  SessionAgentStatus status  = 11;
  // The last thing it was observed doing; unset when nothing has been observed.
  SessionAgentActivity last_activity = 12;
}

enum SessionAgentStatus {
  SESSION_AGENT_STATUS_UNSPECIFIED       = 0;  // nothing has been observed — NOT "idle"
  SESSION_AGENT_STATUS_IDLE              = 1;  // attached, no turn in flight
  SESSION_AGENT_STATUS_RUNNING           = 2;  // prompted, no stop reason yet
  SESSION_AGENT_STATUS_EXECUTING_TOOL    = 3;  // inside a tool call — a refinement of RUNNING
  SESSION_AGENT_STATUS_WAITING_FOR_INPUT = 4;  // blocked on an answer only a human can give
  SESSION_AGENT_STATUS_CONNECTING        = 5;  // the checkout behind it is still being built
  SESSION_AGENT_STATUS_ERROR             = 6;  // it cannot serve prompts; `clone_error` says why
}

message SessionAgentActivity {
  uint64 at_unix_ms = 1;   // never 0 on a populated activity
  string summary    = 2;   // one short line, already truncated for display
}

enum AgentCloneState {
  AGENT_CLONE_STATE_UNSPECIFIED  = 0;
  AGENT_CLONE_STATE_LOCAL        = 1;  // the owning daemon is the facilitating daemon
  AGENT_CLONE_STATE_PROVISIONING = 2;
  AGENT_CLONE_STATE_READY        = 3;
  AGENT_CLONE_STATE_ERROR        = 4;
}
```

`replaces` and `tools` are **snapshotted into the entry at attach**, not re-read from the def on
every use. Editing a YAML def or a registry assistant after attaching therefore does not silently
change what the running session's main agent is allowed to call; detaching and re-attaching does.

### Persistence

`SessionMetadata.specialized_agents: Vec<String>` is **replaced** by:

```rust
/// The session's agent roster. Restored verbatim on resume — a roster is operator intent, not
/// something to re-derive from def sources that may have changed underneath it.
#[serde(default)]
pub agents: Vec<SessionAgentRecord>,
/// The roster revision the persisted `agents` represent.
#[serde(default)]
pub agents_rev: u64,
```

No migration and no back-compat shim: per the repo's standing rule, the old field is removed
outright and a `.session.yaml` carrying it loads with an empty roster. It is recorded in
`docs/dev/changelog/` as a breaking change to session files.

### What an agent is doing

`status` and `last_activity` say whether an attached agent is working. Without them an operator
watching a session could not tell a dispatched agent from an idle one, and the only way to find out
was to prompt it.

**They ride the same whole snapshot.** There is no status *read* RPC, for the reason every roster
read is already a snapshot: a reader that rebuilt its registry from one and then had to correlate a
second stream to learn what each row was doing could show a status for a row it no longer holds.

**`rev` does not move when a status does.** `rev` is the staleness signal for roster *membership* —
an attach or a detach. A status changes on every turn and every tool call, so the daemon republishes
the snapshot at the **same** `rev`. Consumers must therefore adopt a same-`rev` frame's entries.
They must not treat it as a membership change: the addressable ids and the `replaces` union are
fixed at a revision, so announcing an MCP tool-list change on each one would be a notification storm
for a badge.

**The checkout outranks the conversation.** An agent whose clone is still provisioning refuses
prompts, so it reports `CONNECTING` however idle its conversation looks — reporting `IDLE` would
offer an operator an agent that cannot answer. A clone this process has never measured (the shape of
a roster restored from `.session.yaml`) is `CONNECTING` too, for the same reason it is not called
`READY`. A failed clone is `ERROR`. Only once the checkout is usable does the conversation decide.

**Nothing is persisted.** A status is a fact about a running turn loop; written to `.session.yaml`
and read back it would claim a turn is in flight in a process that never started one. A restarted
daemon reports `UNSPECIFIED` for every entry until a signal reaches it.

**A failed turn is `IDLE`, not `ERROR`.** The agent is still attached and still promptable; `ERROR`
is the checkout's. The summary is what says what happened.

Where each signal comes from:

| The loop runs… | How the daemon learns |
|---|---|
| in the daemon, for a local agent | it serves the open, the prompt and the managed tool dispatch itself — the only place `EXECUTING_TOOL` can be told from `RUNNING` |
| on an owning daemon, for a remote agent | the facilitating daemon relays the forwarded turn's frames, so it sees the turn start and end (but not its tool calls) |
| in the jail, for a seeded agent | `ReportAgentConversationState` — the daemon is never asked to open anything, so this is the only possible source |

`ReportAgentConversationState` accepts only the four *conversation* states. `CONNECTING` and `ERROR`
describe the checkout, which the daemon measures itself; a reporter allowed to send them could hide
a broken clone behind a cheerful conversation. `UNSPECIFIED` is refused because a report is by
definition a claim, and accepting it would let a reporter erase what it last said. An `agent_id` the
roster does not hold is `NOT_FOUND`, so a stale in-jail registry cannot put a row on a roster an
operator has emptied. It is authenticated exactly as `ReportAgentCloneState` is, and for the same
reason: the (session, agent) pair is published in the roster broadcast.

Summaries are truncated to 120 characters and collapsed to one line **by the daemon**, not trusted
at the length they arrive. A snapshot past `MAX_CHUNK_FRAME_BYTES` is chunk-framed, and one lost
chunk wedges the call with no error at all — so a `Write`'s `content` must never reach a summary.

## Attaching and detaching

### The RPCs

```proto
service ConnectionService {
  // Attach one agent to a live session. Idempotent on (session, agent_id): re-attaching an already
  // attached agent returns the current roster unchanged and does not bump `rev`.
  rpc AttachSessionAgent(AttachSessionAgentRequest) returns (SessionAgentRoster);
  // Detach one agent. Cancels its open conversations and, when it was the last agent owned by a
  // remote daemon, tears that daemon's clone down.
  rpc DetachSessionAgent(DetachSessionAgentRequest) returns (SessionAgentRoster);
  // The current roster, once.
  rpc ListSessionAgents(ListSessionAgentsRequest) returns (SessionAgentRoster);
  // The roster, now and on every change. The first frame is always the current snapshot, so a late
  // subscriber needs no separate priming read.
  rpc StreamSessionAgents(StreamSessionAgentsRequest) returns (stream SessionAgentRoster);
}

message AttachSessionAgentRequest {
  string session_token       = 1;
  string session_id          = 2;
  string daemon_instance_id  = 3;  // routing, as on ExecuteTool
  string agent_id            = 4;  // "name@daemon_instance_id" — required, qualified
}
```

`ListSubagents` grows the fields a fanned-out picker needs, and its response stops being ambiguous:

```proto
message SubagentInfo {
  string name               = 1;
  string label              = 2;
  string model              = 3;
  string daemon_instance_id = 4;  // NEW — stamped by the serving daemon
  string agent_id           = 5;  // NEW — "name@daemon_instance_id", ready to attach
  repeated string replaces  = 6;  // NEW — so the picker can warn what the main agent loses
  repeated string tools     = 7;  // NEW
}
```

**What it advertises is every def source a name resolves against on the serving daemon** — its
`<tddyhome>/agents/*.yaml` defs *and* its registry assistants (Models & Agents), a registry assistant
winning a name tie. It is answered from the same `resolvable_agent_defs()` an attach resolves against,
and that is the point: an id that is offered but not attachable sends an operator to an
`INVALID_ARGUMENT`, and one that is attachable but not offered is simply invisible. The second is what
happened while `ListSubagents` read only the agents directory — an assistant created in Models &
Agents could be started *as*, and attached by typing its id, but appeared in no picker on any host,
and because a peer's defs are resolved through the peer's `ListSubagents` (step 1 below) it was not
attachable from another host at all.

Two consequences worth stating:

- **Each daemon advertises its own assistants only.** A picker sees another host's assistants because
  it fans `ListSubagents` out, not because any daemon forwards. Both ends of a common room must be
  running a build that advertises them.
- **The response carries no credential.** A registry assistant's provider key is attached on the
  session-start path alone (`agent_def_for_spawn`); `ListSubagents` is answered for every operator.
  Reading the registry can fail, and then the RPC fails rather than answering with the YAML half — a
  partial list is how this bug read as "no agents exist" instead of "one source is broken", and the
  web renders a failing host as its own error row above the picker.

### What attach does

1. **Resolve.** `agent_id` is split. If the daemon part names the facilitating daemon, the def is
   resolved locally through `resolvable_agent_defs()` (YAML + registry assistants). Otherwise the
   facilitating daemon forwards a `ListSubagents` to the named peer over the common room and takes
   the matching row. An unresolvable id is `INVALID_ARGUMENT` naming the id — never a silently
   dropped agent, exactly as `specialized_agents` already behaves.
2. **Provision the clone**, when the owning daemon is remote and holds no clone for this session
   yet. See § Clones below. The entry is published with `clone_ready: false` and republished at
   `clone_ready: true`; provisioning does not block the attach call past its own deadline.
3. **The owning daemon joins the room, via an admission handshake.** B joins
   `session-{session_id}` as `daemon-{B}` and runs its clone's mirror there. B is a *second*
   daemon participant in that room; the facilitating daemon remains the one every file-access RPC
   is addressed to.

   The facilitating daemon is the authority on who may join `session-{session_id}`, so B does
   **not** self-mint its room token. The handshake is:

   1. On `provision_agent_clone`, the facilitating daemon records B in a per-session
      `SessionAdmissionRegistry` and mints a **scoped, short-TTL** (5 min) LiveKit token for
      `session-{session_id}` under the identity `daemon-{B}`. The token, the LiveKit server URL,
      and the room name are forwarded to B inside `StartSessionRequest.agent_clone`
      (`AgentClonePlacement.first_admission_token` / `first_admission_url` /
      `first_admission_room`).
   2. B's `run_clone_mirror` joins with that token and nothing else — it does **not** self-mint,
      because a self-minted token would bypass the registry and its revocation.
   3. **The re-admit loop.** A short-TTL token expires inside a session's lifetime, so the mirror
      wraps its event loop in a reconnection loop: when the session room drops B (token expired,
      LiveKit kicked it), the mirror calls `SessionAdmissionService.AdmitOwningDaemon` over the
      **common room** (the link B never left) for a fresh token, then rejoins `session-{id}` with
      it and re-subscribes the broadcasts. A re-admit that returns `FAILED_PRECONDITION` means the
      facilitating daemon revoked the admission — the last agent this daemon owned detached —
      and the mirror stops cleanly, never as a half-alive checkout nobody mirrors.
   4. **Revocation.** The facilitating daemon revokes an admission in two places: on the last
      detach of an owning daemon's agents (`tear_down_agent_clone` →
      `SessionAdmissionRegistry::revoke`), and on session delete (`revoke_all_for_session`). A
      revoked daemon's next re-admit call is refused with `FAILED_PRECONDITION`, which is what
      makes the mirror exit. The first admit is the facilitating daemon's own act, never the
      `AdmitOwningDaemon` RPC — that RPC only **refreshes** an admission the registry already
      holds, so a daemon the registry does not hold (post-revoke, or never attached) is refused.

   `AdmitOwningDaemon` is served on the facilitating daemon's common-room `daemon-{A}`
   participant, alongside `remote_git.RemoteGitService`. Serving it on the common room (not the
   session room) is deliberate: the re-admit call has to reach A from a B that is *currently
   being kicked from the session room*, and the common room is the link that survives that.

   B's agent surface is **not** served in the session room either — A→B conversation forwarding
   uses the common-room peer path, because `LiveKitParticipant` does not expose its room as a
   shared handle and a second connection under one identity is one LiveKit disconnects.
4. **Bump `rev`, persist, publish.** The new snapshot is written to `.session.yaml` and pushed to
   every `StreamSessionAgents` subscriber — the in-jail `tddy-tools`, the web, and any other
   participant.

If step 2 or 3 fails the attach fails and nothing is left behind: no roster entry, no half-built
clone, no room membership. A failed attach is a session that looks exactly as it did before.

### What detach does

1. Remove the entry, bump `rev`, persist, publish.
2. **Cancel the agent's open conversations.** An in-flight `subagent_prompt` returns an error naming
   the detach. It never hangs and never returns a partial answer as if complete.
3. If it was the last entry owned by a remote daemon: delete that daemon's workspace session (and
   therefore its clone) and leave the room. Teardown follows the discipline
   [remote-managed-worktree.md](remote-managed-worktree.md) § Teardown established — B answering
   "no such session" is idempotent success; B being *unreachable or failing* is a detach failure
   naming the orphaned checkout.

Detaching an agent whose `replaces` set is not claimed by any other entry **restores** those tools
to the main agent at the next enforcement point (§ Tool replacement).

### Seeding at start

`StartSessionRequest.specialized_agents[]` is the same operation as an attach, performed before the
session's agent is spawned. It carries the same qualified ids, and each one is resolved, placed and
clone-provisioned by the steps above — there is one seeding mechanism, not two.

What a start adds is **ordering**, and that is the whole reason a seed cannot just be an attach the
client sends a moment later: a spawn's `--allowedTools` / `--disallowedTools` are fixed at launch
(§ Tool replacement), so an agent named at start must be on the roster *before* the agent process
exists. The order is:

1. Resolve the placement — which daemon holds the authoritative worktree.
2. Resolve every seed reference into a roster record, from the daemon that owns it.
3. Claim a clone for every seeded agent **not** co-located with that worktree.
4. Spawn, with the withdrawn tool set derived from the roster just written.
5. On any failure after step 3, release the clones — a start that does not come up leaves no clone,
   no roster entry and no room membership, the same contract a failed attach keeps.

**The placement is not a gate.** Which host an agent runs on is decided by comparing its owning
daemon against the host holding the authoritative worktree — exactly the comparison an attach makes:

| Where the agent is | What the start does |
|---|---|
| On the host holding the worktree | Records it; it reads that worktree directly, with no clone |
| On any other host | Claims that host one clone for the session, and records the `codebase_session_id` serving it |

For a **split** session the roster write is routed to the codebase host before the session is looked
up, so "the local host" there already means the host holding the worktree: an agent owned by the
codebase host is co-located and gets no clone, while an agent owned by the agent host — or by any
third host — gets one. For a **co-located** session both hosts are the same daemon, and the same
comparison gives the same answers. A peer's agent is admissible either way.

A co-located start writes its `.session.yaml` only once the agent has a pid, so it persists the
roster inline in that file and holds its claimed clones on the start's behalf until the file is on
disk; a start that dies before then releases them. That is an ordering detail of one launch path, not
a second rule.

**A start never waits on the owning daemon.** Seeding an agent on a peer costs that peer's
`workspace` `StartSession`, not a checkout: the clone is provisioned in the background, and a prompt
sent while it is still `provisioning` is refused naming the state (AC33). Session start therefore
never blocks on another host's model warm-up.

An unresolvable reference still fails the start with `INVALID_ARGUMENT` naming it. A session is never
started with a silently dropped agent — which would also be a session whose main agent kept the tools
that agent was meant to take away from it, with only an absent roster to say so.

## Tool replacement, without behaviour

A def's `replaces` is a list of exec-catalog tool names. The **union across the roster** is withdrawn
from the main agent. That is the entire semantics.

Deleted outright, with no replacement:

| Removed rule | Where it lived |
|---|---|
| Replacing `Shell` makes the def the session's *action author* — `request_action` / `list_actions` / `invoke_action` are added to the allowlist automatically | `claude_cli.rs::shell_is_replaced` → `ACTION_TOOLS` |
| At most one def may replace `Shell`; replacing `Write`/`StrReplace`/`Delete` makes the def the session's *coder* | `tddy-sandbox-app/src/config.rs:143-176`, `no-bash-mode.md` |
| "the def must bind the matching internal tool or the session is rejected before spawn" | `config.rs:168-176` |
| `subagent_replaced_tools(name)`'s `"fastcontext"` arm | `tddy-discovery/src/subagent.rs:486` |

**What stays, and why it is not the same thing.** `native_aliases` — `Bash`/`BashOutput`/`KillShell`
for `Shell`, `Edit`/`MultiEdit`/`NotebookEdit` for `Write` — is not a hardcoded *role*; it is what
makes a withdrawal real. Withdrawing `Shell` while leaving native `Bash` callable withdraws nothing,
and the whole point of `replaces` is that the main agent is *forced* to go through the agent. The
mapping is a fact about Claude's native tool names, not a policy about what an agent means.

Three removals change observable behaviour and are worth stating plainly:

- **Replacing `Shell` no longer grants the session-action surface.** A def that wants
  `request_action` / `invoke_action` gets them because the session was configured with them, not as
  a side effect of which tool name appeared in a list.
- **Two agents may now both replace `Shell`.** The main agent has no `Shell` and must name one of
  them. That is unambiguous precisely because agents are addressed by qualified id.
- **An agent may replace a tool it cannot serve.** Nothing validates that `replaces: [SemanticSearch]`
  is backed by a `SEMANTIC_SEARCH` binding — the shipped `fastcontext` def did exactly this
  deliberately, serving it from its `READ`/`GLOB`/`GREP` loop. Enforcing the correspondence would
  have outlawed the one def that motivated the field. What an agent can do is described by its
  `label` and `system_prompt`, which the main agent reads in the context appendix.

### Enforced at two layers, because attach is live

`--allowedTools` / `--disallowedTools` are fixed when `claude` is spawned. An agent attached at
minute forty cannot retroactively remove `Grep` from a process launched at minute zero. So
withdrawal is enforced twice:

| Layer | When | What it covers |
|---|---|---|
| **Spawn allowlist** (existing) | Session start and every resume | The roster as it stands at launch. `build_claude_allowlist` / `build_claude_disallowlist` are computed from the persisted roster instead of the start request's names. |
| **Runtime refusal** (new) | Every exec-tool call | The *live* roster. `tddy-tools` refuses a call to a currently-replaced tool with an error naming the agent that replaced it and the qualified id to address instead. |

The first layer is what makes a **seed** real. The roster the allowlist is computed from is written
before the spawn (§ Seeding at start), so an agent named at start withdraws its tools at launch
rather than at the next resume — including one owned by another daemon.

The second layer is what makes live attach real. In a managed-codebase session the main agent's file
tools **are** `mcp__tddy-tools__*`, so the refusal happens on the path the call already takes — no
new interception point, no new process. The refusal is hard: there is no fallback to running the
tool anyway.

```
Grep failed: withdrawn from this session — the tool is served by agent "explorer@ws-01".
Call subagent_new_session { agent: "explorer@ws-01" } and prompt it instead.
```

For a **non-managed** session the main agent has native filesystem tools that `tddy-tools` never
sees, so live withdrawal there is advisory until relaunch. Attaching an agent with a non-empty
`replaces` to a non-managed session is therefore **refused**, rather than accepted and quietly
unenforced.

Three session shapes enforce a withdrawal, and it is worth being exact about the third, because the
attach for a split session is routed to it:

| Shape | Why the tool is refused on the path the call already takes |
|---|---|
| **Managed codebase** | The jail is what puts the agent's file tools at `mcp__tddy-tools__*`. |
| **A split session's agent half** | No jail, but no codebase either: the spawn hard-disables every native filesystem tool, so the proxy is the only route it has. |
| **A split session's codebase half** | The `workspace` session keeps the roster and receives the attach. It runs no agent loop; the withdrawal is enforced on the agent host, which the session names. |

The third row turns on the **pairing**, not the session type. A `workspace` session is also what an
operator's standalone checkout is, and what an agent clone's mirror is; neither has an agent anywhere
whose tools a roster could take away. So the check reads the persisted back-pointer
(`agent_daemon_instance_id` / `agent_session_id`, see
[remote-managed-worktree.md](remote-managed-worktree.md) § `SessionMetadata`) and refuses a
tool-replacing agent on a `workspace` session no agent works in — the same refusal a non-managed
session gets, for the same reason.

A `workspace` session created before that back-pointer existed carries none, so an agent that
replaces tools is refused on it until the split session is restarted. Guessing the pairing from the
session type is exactly the unenforced acceptance this refusal exists to prevent.

## Clones

### One clone per (session, remote daemon)

Attaching `explorer@B` and `linter@B` to the same session produces **one** checkout on B, shared by
both. Two agents on one host reading the same tree is the common case; a checkout each would
multiply disk and sync cost for no isolation the read-only model needs.

The clone is a `workspace` session on B — the same primitive
[remote-managed-worktree.md](remote-managed-worktree.md) already uses, and for the same reasons: B
knows how to provision a project by clone when it does not have one, how to build a worktree for a
branch, how to report and delete it, and an operator can see and remove it like any other session.
Its id is minted by A and sent as `requested_session_id`, so a forward that times out still leaves A
able to name — and therefore tear down — whatever B built.

**It is never the project directory and never a worktree A owns.** `repo_path`-style reuse would put
a second agent's `Shell` in the operator's own checkout.

### Kept current from the room

B joins `session-{session_id}`, and from that room it runs the
[session worktree sync](session-worktree-sync.md) client algorithm **in-process**:

- a `commit` activity event → fetch `refs/tddy/session/{id}/wip` over the existing
  `tddy-remote-git-repo` transport, `reset --hard <wip>^`, `read-tree -u --reset <wip>`;
- an edit activity → `StreamAgentActivityDelta` by `call_id`, applied with `git apply` in `seq`
  order;
- any divergence → reconcile from the WIP ref and log at `error`.

So a remote agent sees the main agent's **uncommitted** work, which is the only thing that makes it
useful for review or search during a turn. Nothing new is invented here: this is why the worktree
sync landed first.

Sync is **one-way**. The clone is a mirror, and the marker file
`.tddy-session-sync.json` that the standalone client writes is written here too, so a checkout that
was a mirror cannot later be mistaken for a workspace.

### Reads are local; writes proxy

A remote agent's turn loop runs in B's daemon process, with a `CodebaseAccess::Managed` dispatch
that splits on the tool:

| Tools | Served by |
|---|---|
| `READ`, `GLOB`, `GREP`, `SEMANTIC_SEARCH`, `READ_LINTS` | B's own clone, locally. No round trip. |
| `WRITE`, `STR_REPLACE`, `DELETE`, `SHELL`, `AWAIT` | **Daemon A**, over `StreamExecuteTool`, against the authoritative worktree. |

There is exactly one worktree that counts, and it is A's. A mutation applied to B's clone would be
overwritten by the next sync tick and would never reach the session's branch — so it proxies, and
the sync tick that follows brings the result back to B like any other change.

`SHELL` proxying is the sharp edge and is documented as such: a build a remote agent runs executes on
**A**, not on the host whose toolchain motivated attaching it. Running it on B would mean writes
landing in a mirror. Turning that into "the agent can run builds on its own host" needs write-back,
which is a separate feature; it is a non-goal here and recorded in `docs/dev/TODO.md`.

### Clone state, pushed not polled

Readiness, the checkout's path and every reconcile are facts only the daemon holding the checkout
can state. The facilitating daemon owns the roster and answers every read of it, so it has to be
*told*: a poll would have it deciding an entry is ready from the outside, which is how a prompt
gets served from an empty tree. Two RPCs carry this, both additions to `ConnectionService`:

```proto
// The facilitating daemon forwards this inside StartSessionRequest when it commissions a clone
// on an owning daemon. The owning daemon reads the first-admission fields to join the session
// room under the handshake (§ "What attach does" step 3) rather than self-minting.
message AgentClonePlacement {
  string codebase_session_id        = 1;  // the workspace session id A minted for the clone
  string facilitating_daemon_url     = 2;  // A's remote-git URL for `git clone` (AC37)
  string session_token              = 3;  // the owning daemon's access token for A's RPCs
  string first_admission_token      = 4;  // scoped, short-TTL token A minted for the handshake
  string first_admission_url        = 5;  // the LiveKit server to join with that token
  string first_admission_room       = 6;  // the session room name (`session-{id}`)
}

// The owning daemon reports its clone's state back to the facilitating daemon, which republishes
// it on the roster. Refused for a clone this daemon did not itself ask that daemon for, and for one
// naming a different checkout — the report is what authorizes an entry to start serving prompts,
// so accepting an unknown one would let any room participant mark an agent ready.
rpc ReportAgentCloneState(ReportAgentCloneStateRequest) returns (ReportAgentCloneStateResponse);
```

`ReportAgentCloneState` is pushed: the owning daemon calls it on every state transition
(`PROVISIONING → READY`, or `→ ERROR`), and the facilitating daemon republishes the entry on
`session.agents`. A `READY` report is the gate a prompt checks — a prompt for an entry whose
clone is not `READY` is refused naming the state, never queued.

## Invoking an agent

Unchanged in shape, generalized in reach. The main agent uses the existing MCP surface:

```
subagent_new_session { agent: "explorer@ws-01" }   → { sessionId }
subagent_prompt      { sessionId, prompt: [...] }  → { stopReason, content }
subagent_cancel      { sessionId }
subagent_status      { }                           → { sessionId, appliedRev, agents:[…] }
subagent_status      { agent, waitFor: "ready" }   → …the same, once it can be prompted
```

`tddy-tools` resolves `agent` against its **live roster**, not `TDDY_SUBAGENT`:

- **Local entry** → a `SpecializedSubagentSession` built from the entry's def, as today.
- **Remote entry** → an `OpenAgentConversation` / `PromptAgentConversation` / `CancelAgentConversation`
  RPC to the **facilitating daemon**, which forwards it to the owning daemon in the session room. The
  in-jail transport does not change: `tddy-tools` still speaks only to daemon A, over whichever of
  `SandboxIpc` / `LiveKit` / `DaemonHttp` it already detected.

`TDDY_SUBAGENT`'s role as a default agent name is **removed**. `subagent_new_session` without an
`agent` field is an error listing the roster's ids — with an unbounded roster there is no defensible
default, and picking the first entry would make the main agent's choice depend on attach order.

### `subagent_status`

What every attached agent is doing right now, read off the live roster. Deliberately **not**
`subagent_list`, which enumerates this process's *conversations* and their token cost: an agent the
main agent could address but has never opened a conversation with appears in `subagent_status` and
nowhere else, which is what makes it answerable *before* any work has been dispatched.

Each row carries the entry's id, label, model, owning daemon, `replaces`, clone state and status,
plus `lastActivity` when there is one and this process's conversations with that agent. `status` is
reported as a word, and `UNSPECIFIED` becomes `"unknown"` — never `"idle"`, because telling the main
agent an agent is idle would have it dispatch work to a row the daemon cannot currently account for.

A roster that has gone **dark** still answers, carrying `refusal`: the rows it last knew about plus
the reason none of them can be addressed is strictly more useful than an error, which would answer
"what are my agents doing?" with nothing at all. `appliedRev` is `null` when no frame has ever
arrived, which is a different state from a daemon that published an empty roster at rev 0.

A jail-run conversation also **reports itself** through `ReportAgentConversationState` at open,
prompt, turn end and cancel (§ What an agent is doing), so the row this tool reads is populated for
a seeded agent the daemon never opened.

#### Waiting, rather than polling

Reading the roster answers "can this agent be prompted?" at one instant. The main agent's actual
question is "tell me when it can" — an attach returns before the agent is usable, and a prompt sent
while its checkout is provisioning is refused naming the clone state (AC33). Polling costs a whole
turn per look, which is the most expensive way to wait that exists.

`subagent_status { agent, waitFor: "ready", timeoutMs }` parks until the named agent stops being
`connecting`, then returns **the same report a plain read returns**, plus `timedOut`. `agent` is the
wait's target, not a filter: the report still carries every row, so nothing about the plain read
changes when a wait is asked for.

- **`waitFor` requires `agent`.** "Ready" across an unbounded roster is neither "all of them" nor
  "any of them" in a way the caller could have meant, and guessing either would make the answer
  depend on attach order, which the main agent cannot see.
- **`error` settles the wait.** Promptable is `status ∉ { connecting, error }`, but only one of the
  two is worth waiting through: a failed checkout is not something waiting fixes, and parking to the
  deadline would report the same failure later and call it a timeout.
- **`unknown` settles it too.** It is what a restarted daemon reports for an agent whose checkout is
  on disk and perfectly promptable, so treating it as not-ready would park every wait on such a
  session until its deadline.
- **A detach settles it.** What the wait is waiting for cannot happen once the roster stops carrying
  the row, and the report says so by not listing it — better than an error, which would discard
  every other row to report one.
- **A deadline is always in force.** `timeoutMs` defaults to 30s and is capped at 120s; one tool
  call holding the main agent for ten minutes is worse than one saying "still connecting". Expiry
  returns the current rows with `timedOut: true` rather than an error, because the last known status
  is the actionable half of the answer.

The wait wakes on the roster's **snapshot generation**, not on `rev`. A status change republishes at
the revision already in force — `rev` moves on attach and detach, not on a badge — so a wait
watching revisions alone would park until its deadline on precisely the transition it exists to
catch.

### The roster stream

`tddy-tools --mcp` opens `StreamSessionAgents` at startup and holds it for the process lifetime.
Every snapshot at a **new** `rev` rebuilds `SubagentRegistry` and emits an MCP
`notifications/tools/list_changed`, so the main agent's own tool listing reflects the roster without
a restart.

A snapshot at the `rev` **already applied** is adopted for its entries and announces nothing. That
is the frame carrying a status change (§ What an agent is doing): the agents are identical by
construction, so nothing can newly need cancelling and no tool list has changed, but what those
agents are *doing* is exactly what the frame exists to deliver. A frame *older* than the one in
force stays ignored — applying it would resurrect a detached agent.

`TDDY_SUBAGENTS_JSON` remains, demoted to a **seed**: it makes the roster usable in the window
between spawn and the stream's first frame, and it is what `tddy-sandbox-app` — which has no daemon
in the loop — continues to run on. A `tddy-tools` that cannot open the stream **fails loudly** rather
than falling back to the seed forever: a registry frozen at the seed answers for agents that were
detached, and silently running the wrong roster is the failure this design exists to prevent.

The stream is the first long-lived server stream over `SandboxIpc`. That transport opens a fresh
`UnixStream` per dispatch today and its `call_server_stream` carries no deadline, so the roster
stream gets its own connection and an explicit reconnect-on-drop with backoff, reported at `error`
on give-up.

## Removing the hardcoded agents

Every one of these is deleted, and `fastcontext` appears in no Rust identifier, no default, and no
shipped file afterwards:

| Site | Change |
|---|---|
| `tddy-discovery/src/agent_def.rs` | `builtin_fastcontext_def()` and `builtin_agent_defs()` deleted; `resolve_agent_defs(dir)` returns exactly what `dir` holds |
| `tddy-discovery/src/subagent.rs` | `subagent_replaced_tools(name)` and `resolve_replaced_tools(name, csv)` deleted — the roster is the only source of a replaced set |
| `tddy-discovery/src/backend.rs` | `FastContextBackend` → `SpecializedAgentBackend`; `name()` returns the def's own name; the `microsoft/FastContext-1.0-4B-RL` and `:30000` defaults go |
| `tddy-coder/src/run.rs` | `--fastcontext-url` / `--fastcontext-model` / `--fastcontext-max-turns` CLI flags and their `Config` fields deleted — a def's `base_url`/`model`/`max_turns` are the only source, as `specialized-subagents.md` AC12 already requires of `tddy-sandbox-app`. (`Args.agent` already carries no static allowlist.) |
| `tddy-sandbox/src/context_dir.rs` | appendix text stops naming `fastcontext`; it renders the live roster's qualified ids |
| `tddy-tools/src/server.rs` | `TDDY_SUBAGENT` default-agent fallback removed |
| `tddy-sandbox-app/src/config.rs` | action-author / coder validation removed; `specialized_agents:` resolves only against `<tddyhome>/agents` |

**There is no seeded replacement file.** An operator who wants the old behaviour writes
`<tddyhome>/agents/fastcontext.yaml` themselves, or creates the assistant in Models & Agents; the
shape is unchanged and documented in [specialized-subagents.md](../coder/specialized-subagents.md).
Shipping a seed file would reintroduce exactly the hardcoded default this asks to remove, one
directory further out.

A session started with an empty roster and no `--agent` is a perfectly ordinary session — the same
one every non-managed session already is.

## Web UI

### Create-session picker (existing, widened)

`CreateSessionPane`'s managed-codebase multi-select stops listing one daemon's subagents and lists
**every common-room daemon's**, fanned out client-side exactly as the sessions drawer does for
`ListSessions` and the Models & Agents screen does for assistants. Each row shows its owning daemon;
the value threaded into `specializedAgents` is the qualified `agent_id`. One daemon failing to answer
costs one error row, never the picker.

The picker is offered on **every** codebase placement, a split session included. Where an agent runs
decides how the session is split across hosts, not whether it may be selected: a split session's seed
is resolved and recorded on the codebase host, which is the host that holds the roster
(§ Seeding at start). The Semantic index toggle beside it is offered on the same terms, and for the
same reason (remote-managed-worktree.md § What a split session cannot also ask for).

### The Agents tab — a tree of everything working for one agent

The session inspector's **Agents** tab is a tree rooted at the session's own **main agent**. Two
populations hang beneath it, and the tab is the only place either is listed:

| | Managed roster agent | Subagent session |
|---|---|---|
| What it is | A specialized agent the facilitating daemon runs a loop for | A `claude-cli` / `cursor-cli` session the main agent spawned |
| Arrives on | `StreamSessionAgents` | `ListSessions`, linked by `orchestrator_session_id` |
| Status from | `SessionAgentEntry.status` | The inferred `SessionEntry.agent_status` ([agent-session-status.md](agent-session-status.md)) |
| Row affords | **Detach** | **Switch** — focuses that session's runtime |

A subagent session nests its own roster agents and its own subagents in turn, so a spawn chain reads
as a chain rather than as a row of siblings. Every row states which kind it is; a label cannot say,
and the two afford different actions.

**One badge for both.** The proto ships one `SessionAgentStatus` for a roster agent and an agent
session alike, and the tab renders them through one vocabulary. The badge is always present,
`unknown` included: a row with no badge and a row whose daemon has nothing to say look identical
otherwise. `unknown` is never spelled "idle" — an operator reads "idle" as "free, ready for work",
which is a different claim from "nobody here knows". A subagent whose session type the daemon does
not tail (`tool`, `workspace`) reports `unknown` honestly rather than being assigned a state.

**Rows and their contents.** A roster row shows qualified id, label, model, owning daemon, the tools
it replaces, and its clone state — `provisioning` / `ready` / `error`, never a blank. A subagent row
shows its session id, agent and model. Both carry a last-activity line reading "<summary> · 4m ago"
that **ages on its own**, ticked once a minute for the whole tree: an idle agent produces no frames,
so a line that only aged on a frame would read "just now" for the rest of the session. A stamp in the
future reads "just now" rather than a negative age, because two hosts' clocks disagree by seconds
routinely. A row nothing has been observed of shows no line at all — one reserved for a history that
does not exist reads as a row that lost one.

**A collapsed subagent costs nothing.** The tab stays open for the life of the inspector, so
subscribing every descendant on mount would hold one daemon stream per subagent for that whole time.
A subagent opens its roster stream only while expanded, and its *own* status needs no stream, so a
collapsed row still says what it is doing. An expanded subagent reads its roster on **its own**
codebase half — it can be split independently of its parent, and the agent half would answer with an
empty list beside the real one.

**What the tree cannot show.** It is folded from the session list the browser holds, so a subagent
spawned on a host the browser is not aggregating is not a node of it. An orphan — a session whose
orchestrator is absent — is dropped rather than promoted to the root, where it would claim the main
agent spawned it. Orchestrator links that form a cycle terminate the branch instead of repeating it;
the list is assembled from several hosts' answers, so a cycle is a shape to survive, not one to
assume away.

**Flows.** **Add agent** opens the same fanned-out picker as the create pane and warns which tools
the main agent loses before confirming. **Detach** is inline and confirms when the entry is the last
one owned by a remote daemon, because that detach deletes a checkout on another host — judged
against the roster the entry belongs to, which for a nested row is that subagent's roster, not the
root's. A subagent row has no Detach: there is no roster entry behind it.

**Failure is never silence.** Four states are distinct for the tab as a whole — not connected,
loading, read failed, genuinely empty — and an expanded subagent whose own roster could not be read
says why on its row. An unreadable roster is not an empty one, at any depth.

## Acceptance Criteria

### Roster — the wire

1. `AttachSessionAgent` with a qualified `agent_id` naming a def on the facilitating daemon adds one
   entry, bumps `rev` by exactly one, and returns the full new roster.
2. Attaching the **same** `agent_id` twice returns the roster unchanged and does **not** bump `rev`.
3. Attaching an `agent_id` that resolves on **no** daemon is `INVALID_ARGUMENT` naming the id; the
   roster and `rev` are unchanged.
4. An unqualified `agent_id` (no `@`) is `INVALID_ARGUMENT`. There is no "assume the local daemon"
   reading.
5. There is **no arity limit**: attaching ten agents yields a ten-entry roster, and every entry is
   addressable.
6. `DetachSessionAgent` removes exactly the named entry, bumps `rev`, and leaves every other entry
   in place and in order.
7. Detaching an `agent_id` not in the roster is `NOT_FOUND`, not a silent success.
8. `ListSessionAgents` returns the same snapshot `StreamSessionAgents`' first frame carries.
9. `StreamSessionAgents` emits the current snapshot **immediately** on subscribe, then one snapshot
   per subsequent `rev` change, and none for a no-op attach.
10. The roster survives a daemon restart: `.session.yaml`'s `agents` / `agents_rev` are restored
    verbatim, and `rev` continues from the persisted value rather than restarting at zero.
11. A `.session.yaml` written before this change (carrying `specialized_agents`) loads with an
    **empty** roster and no error.
12. Every roster RPC is refused for a caller whose `session_token` does not resolve, with the same
    `UNAUTHENTICATED` / `PERMISSION_DENIED` treatment every other `ConnectionService` RPC gives, and
    **before** any peer is contacted or any clone is provisioned.

### Roster — the live registry

13. `tddy-tools --mcp` builds its initial `SubagentRegistry` from `TDDY_SUBAGENTS_JSON` and replaces
    it wholesale with the first `StreamSessionAgents` frame.
14. A roster frame adding an agent makes `subagent_new_session { agent: "<new id>" }` succeed
    **without** the process restarting.
15. A roster frame removing an agent makes `subagent_new_session` for it fail with an error naming
    the id, and **cancels** any conversation already open with it.
16. Each roster frame emits exactly one MCP `notifications/tools/list_changed`.
17. `tddy-tools` that cannot open or maintain `StreamSessionAgents` reports the failure at `error`
    and refuses subagent calls — it does not keep serving the seed indefinitely.
18. `subagent_new_session` with no `agent` field is an error listing the roster's qualified ids;
    `TDDY_SUBAGENT` is not consulted.

### Tool replacement

19. The union of every roster entry's `replaces` is what the main agent loses — two agents each
    replacing one tool withdraw both.
20. A call to a currently-replaced exec tool is refused by `tddy-tools` with a message naming the
    replacing agent's qualified id. The tool does **not** run.
21. Detaching the only agent replacing a tool makes that tool callable again, in the same process.
22. An agent replacing `Shell` does **not** gain the session-action tools, and **two** agents may
    both replace `Shell` — neither is refused. The native `Bash` family stays hard-disabled, because
    that is what makes the withdrawal real.
23. An agent whose `replaces` names a tool it does not bind attaches successfully.
24. Attaching an agent with a non-empty `replaces` to a **non-managed** session is refused, naming
    the reason.
25. `build_claude_allowlist` / `build_claude_disallowlist` are computed from the **persisted roster**
    at spawn and at resume, so a session resumed after an attach launches with the tool withdrawn.
    For a **split** session the persisted roster lives on the codebase daemon, so the resume reads
    it from there and is **refused** when that host cannot be reached — an empty roster read from an
    unreachable peer is indistinguishable from no agents, and would relaunch the main agent holding
    the tool the operator gave away (remote-managed-worktree.md § Resume).

### Remote agents

26. Attaching an agent owned by another daemon resolves its def **from that daemon**, and the entry
    records that daemon and the clone serving it.
27. The owning daemon joins `session-{session_id}` and serves its agent surface there; the
    facilitating daemon remains the identity every file-access RPC is addressed to.
28. `subagent_prompt` to a remote agent runs the loop **on the owning daemon** and returns the same
    `{stopReason, content}` shape a local agent returns — the main agent cannot tell them apart.
29. Two agents owned by the **same** remote daemon share **one** clone.
30. Two agents owned by **different** remote daemons get one clone each.
31. A remote agent's `READ`/`GLOB`/`GREP` is served from its own clone, with no `ExecuteTool` to the
    facilitating daemon.
32. A remote agent's `WRITE` / `STR_REPLACE` / `DELETE` / `SHELL` is proxied to the facilitating
    daemon and lands in the **authoritative** worktree, not the clone.
33. A prompt sent to an agent whose clone is still provisioning is refused naming the clone state —
    never queued and never served from an empty checkout.
34. An owning daemon that is unreachable at attach fails the attach with no entry, no clone and no
    room membership left behind.
35. An owning daemon that becomes unreachable mid-session fails that agent's prompts with an error
    naming the daemon; the rest of the roster keeps working.

### Clones

36. The clone is a `workspace` session on the owning daemon whose worktree is **not** the project
    directory and **not** any worktree the facilitating daemon owns.
37. The owning daemon provisions the project by clone when it does not already have it.
38. A `Write` by the main agent appears in the remote clone — with identical bytes and **without a
    commit** — so a remote agent reads in-flight work.
39. A `git commit` in the session moves the clone's `HEAD` to the same sha.
40. A clone that has diverged is reconciled from `refs/tddy/session/{id}/wip` and the divergence is
    logged at `error`.
41. Detaching the last agent owned by a daemon deletes that daemon's workspace session and its
    checkout.
42. A detach whose peer answers "no such session" **succeeds**; a detach whose peer is unreachable
    **fails**, naming the orphaned checkout.
43. Deleting the session deletes every remote clone it created.

### No hardcoded agents

44. `resolve_agent_defs` over an **empty** directory returns an empty list — there is no builtin.
45. No Rust identifier, default value or shipped file contains `fastcontext`.
46. `create_backend` builds a backend whose model, base URL and turn budget come **only** from the
    resolved def; the `--fastcontext-*` flags no longer exist and cannot override it.
47. A session started with an empty roster starts normally, with the full native tool set.

### Web

48. The create-session subagent picker lists agents from every common-room daemon, each labelled
    with its owning daemon, and sends qualified ids.
49. One daemon failing to answer the picker's fan-out costs one error row, not the picker.
50. The Agents tab renders the live roster and updates on an attach made elsewhere, with no
    manual refresh.
51. The tab's Add-agent flow states which tools the main agent will lose before confirming.
52. Detaching the last agent of a remote daemon asks for confirmation naming the host whose
    checkout will be deleted, judged against the roster that entry belongs to.
53. Not-connected, loading, read-failed and empty are four distinct rendered states.
53a. The tab is a tree: roster agents and spawned subagent sessions render as children of the
    session's main agent, and a subagent's own roster agents and subagents render beneath it.
53b. A managed row and a subagent row draw their badge from one vocabulary, and each row states
    which of the two it is.
53c. A collapsed subagent opens no roster stream; an expanded one reads its own codebase half.
53d. A subagent row offers Switch and no Detach.
53e. An orchestrator cycle terminates, a self-reference is dropped, and an orphan is not promoted
    to the root.
53f. An expanded subagent whose roster could not be read says why, on its own row.

### Seeding at start

54. A session started with an agent owned by a **peer** succeeds, and the roster records that daemon
    and the clone serving it — on a co-located placement as much as a split one.
55. A **split** session started with an agent owned by the **codebase host** succeeds with **no
    clone**: that agent reads the authoritative worktree directly.
56. A split session started with two agents owned by the same third host gets **one** clone, and one
    started with agents on two different hosts gets one clone each — the seed places an agent by the
    same rule an attach does.
57. A seeded agent's `replaces` is withdrawn from the main agent **at launch**, not at the first
    resume: the spawn's tool set is derived from the roster written before it.
58. A start that fails after clones were claimed leaves no clone, no roster entry and no room
    membership on any host.
59. An unresolvable seed reference fails the start with `INVALID_ARGUMENT` naming the reference, and
    no session is created.
60. Session start never blocks on the owning daemon's model; a prompt to a seeded agent whose clone
    is still provisioning is refused naming the clone state (AC33).

### Waiting for an agent to become promptable

61. `subagent_status { agent, waitFor: "ready" }` returns as soon as a later frame reports the agent
    out of `connecting`, without waiting out the deadline.
62. It returns when that frame arrives at the revision **already applied**, which is how a status
    change is published — a wait driven by `rev` alone would miss every one of them.
63. An agent that is already promptable returns immediately, `unknown` included.
64. An agent whose clone failed settles the wait and is reported `error`; waiting does not fix a
    failed checkout, and running to the deadline would report it as a timeout instead.
65. An agent detached while the wait is parked settles it, and the report simply no longer lists
    that agent — every other row survives.
66. Reaching the deadline returns the current rows with `timedOut: true`, never an error.
67. No wait parks longer than 120s however large `timeoutMs` is.
68. `waitFor` without `agent` is refused naming the field; `waitFor` with an unrecognised condition
    is refused naming what it takes; a `timeoutMs` that is not a whole number is refused rather than
    silently replaced by the default.
69. A read that asked for no wait carries no `timedOut` at all, and naming an `agent` without
    `waitFor` still reports every attached agent.

## Design decisions

### Qualified ids over an opaque attachment id

An opaque `agent_attachment_id` makes collisions impossible by construction, but it is not a string
the main agent can be *told* — the sandbox appendix would have to render a lookup table, and every
log line would need a resolve to be readable. `name@daemon_instance_id` is unique for the same reason
LiveKit participant identities are, and it is the same token in the roster, the appendix, the RPC and
the error message.

Its cost is real and accepted: re-homing a daemon under a new instance id invalidates persisted
roster entries. They fail to resolve loudly at the next prompt rather than silently routing
elsewhere.

### The roster lives on the facilitating daemon, not in the room

The room is the *transport* for remote agents, not the store. A LiveKit room's metadata is already
the worktree summary, it is size-bounded, and it disappears when the room closes — while a roster
must survive a daemon restart and be readable by a session's own `.session.yaml`. Putting the roster
in room metadata would have made "which agents are attached" unanswerable for a session whose room
is not currently open.

### Whole snapshots, never diffs

A diff stream is smaller and strictly worse here: the consumer that matters is an MCP registry whose
disagreement with the daemon is silent and wrong, not merely stale. A snapshot cannot drift.

### Reads local, writes proxied

Chosen over a read-write clone with write-back because the session has exactly one authoritative
worktree and a second writer needs a conflict story that nothing in this system has. It is also the
only option that keeps the *interesting* half — fast local reads against a real checkout on the host
with the model — while leaving the branch's contents attributable to one worktree.

### `replaces` keeps no per-tool meaning

The `Shell` and `Write` special cases encoded a policy (no-bash mode) into the mechanism that binds
agents to tools. With an unbounded roster of operator-defined agents the policy has no defensible
default, and the mechanism has to be plain. What no-bash-mode wanted is still expressible — an agent
that replaces `Shell` — it is just no longer implied by which tool name appears in a list.

### Runtime refusal rather than relaunch on attach

Relaunching the agent CLI to change its allowlist would interrupt the operator's conversation, which
is the thing they attached an agent in the middle of. Refusing the withdrawn tool at the point it is
called achieves the same guarantee on the path the call already takes.

## Non-goals

- **Write-back from a remote clone.** Mutations proxy; they do not reconcile. A remote agent cannot
  build on its own host.
- **Autonomous agents.** Roster agents are **passive**: they act only when the main agent prompts
  them. They do not react to `session.activity`, do not hold their own turn loop between prompts,
  and do not address each other.
- **Agent-to-agent addressing.** Only the main agent addresses the roster.
- **Multi-hop.** A facilitating daemon addresses an owning daemon directly; no chains.
- **Per-agent clone isolation on one host.** Agents owned by one daemon share its clone.
- **Migrating `specialized_agents` in existing `.session.yaml` files.** They load with an empty
  roster.
- **A seeded `fastcontext.yaml`.** Removing the builtin means removing it, not relocating it.
- **Rosters on `workspace` sessions.** A session with no agent has no roster.
- **Re-resolving a def after attach.** Editing a def does not change a running session's roster.

## Related documentation

- [Specialized subagents](../coder/specialized-subagents.md) — the def format and the start-time
  model this supersedes
- [Models & Agents](../web/models-and-agents.md) — the registry that is one of the two def sources
- [Session rooms](session-room.md) — the room remote daemons join
- [Session worktree sync](session-worktree-sync.md) — the sync algorithm a remote clone runs
- [Remote managed worktree](remote-managed-worktree.md) — the `workspace`-session and tool-proxy
  primitives reused here
- [Managed-codebase mode + discovery subagents](../coder/managed-codebase-subagents.md) — tool
  replacement's original contract
- [No-bash mode](../coder/no-bash-mode.md) — the policy whose hardcoded encoding is removed
