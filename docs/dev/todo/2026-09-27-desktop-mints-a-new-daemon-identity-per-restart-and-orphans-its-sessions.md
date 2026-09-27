# 2026-09-27 — The desktop app mints a new daemon identity on every restart, and every session recorded against the old one is permanently unroutable

**Category:** Known defect (a shipped configuration line, plus a missing durable-id indirection that
the rest of the codebase already has)
**Source:** Investigating why **Resume** appeared to do nothing for session
`01a0e1dc-85a6-7a01-8e39-0d63c520215a` under `~/.tddy`, with no error shown to the operator.
**Why it was deferred:** found while wrapping [#549](https://github.com/uppin/tddy-coder/pull/549)
(the ripgrep symlink/dylib grant fix). The two share no file, no package and no test — folding a
session-identity change into a two-line sandbox fix would have made #549 unreviewable. Nothing
about the fix depends on #549 landing; it was a scope call, not a blocked one. **Do not treat this
as waiting on anything.**

---

## TL;DR for whoever picks this up

`daemon_instance_id_append_startup_timestamp: true` **ships** in `desktop.yaml.production`. It makes
the daemon answer to a different routing id on every launch. Session metadata — and, worse, the
**primary key of every roster agent** — persists that per-run routing id. So one restart of the
desktop app permanently orphans every split session and every roster entry on the machine. The
codebase already solved this exact problem for the host registry
(`local_base_instance_id_for_config`); sessions and the agent roster are the places that missed it.

Three fixes, separable, in the order they buy the most: **(b)** stop shipping the flag on, **(a)**
persist the durable id and resolve at use, **(c)** make Resume refuse a route it does not have.
Details in *What the fix is* below.

---

## The evidence

### The ids

```
recorded in the session:  LT-R1VXTH2V6H-1790495455982
the daemon now running:   LT-R1VXTH2V6H-1790497976955      (~42 minutes later)
```

`LT-R1VXTH2V6H` is the hostname; the suffix is the process's startup wall-clock millisecond.

### What the session files hold

The session is a **split placement**: a `claude-cli` agent half paired with a `workspace` half that
owns the worktree. Both halves record the routing id, and so does every roster entry.

`~/.tddy/sessions/01a0e1dc-85a6-7a01-8e39-0d63c520215a/.session.yaml` (agent half):

```yaml
session_type: claude-cli
codebase_daemon_instance_id: LT-R1VXTH2V6H-1790495455982
codebase_session_id: 01a0e1dc-85a6-7a01-8e39-0d7c47b28731
```

`~/.tddy/sessions/01a0e1dc-85a6-7a01-8e39-0d7c47b28731/.session.yaml` (workspace half):

```yaml
session_type: workspace
agent_daemon_instance_id: LT-R1VXTH2V6H-1790495455982
agents:
  - agent_id: FastContext@LT-R1VXTH2V6H-1790495455982
    name: FastContext
    daemon_instance_id: LT-R1VXTH2V6H-1790495455982
  - agent_id: Gemma Local Coder@LT-R1VXTH2V6H-1790495455982
    name: Gemma Local Coder
    daemon_instance_id: LT-R1VXTH2V6H-1790495455982
```

**`agent_id` is the thing to notice.** It is not a label — it is the addressable identity, minted by
`qualified_agent_id` (`packages/tddy-session-agents/src/agent_records.rs:43-51`) as
`name@daemon_instance_id`, parsed back by `tddy_core::AgentId::parse`, and split on `@` at
`packages/tddy-session-agents/src/session_agent_roster.rs:427-428`. So the blast radius is not the
two placement fields: it is **every roster agent's primary key**, in ordinary co-located sessions
too, not only split ones.

### The rejection in the log

Immediately after each of the two `ResumeSession` dispatches (11:33:02.647 and 11:33:06.423):

```
[tddy_daemon_livekit::peer_routing] ListSessionAgents: rejected daemon routing:
  unknown or not connected daemon_instance_id "LT-R1VXTH2V6H-1790495455982":
  peer is not in the current eligible daemon list
```

The daemon is on the same machine and perfectly healthy. Nothing can make that id eligible again —
the suffix is a wall-clock reading, so the orphaning is permanent, not "until the next restart".

---

## The mechanism, end to end

1. **The flag ships on.** `desktop.yaml.production:108` sets
   `daemon_instance_id_append_startup_timestamp: true`, and `dev.desktop.yaml:45` matches it. The
   kernel default is `false` (`packages/tddy-daemon-kernel/src/config.rs:496`), and
   `dev.daemon.yaml:37` sets it `false` with a comment at line 35 saying to turn it on only "if
   running multiple daemons simultaneously".

2. **It rewrites the routing id per process.** `local_instance_id_for_config`
   (`packages/tddy-daemon-kernel/src/daemon_identity.rs:76-84`) appends
   `process_startup_unix_ms_suffix()` to the base id.

3. **Durable state persists that routing id.**
   - `codebase_daemon_instance_id` — written at
     `packages/tddy-session-lifecycle/src/connection_service/svc_spawn_split_agent.rs:432`
   - `agent_daemon_instance_id` — written at
     `packages/tddy-session-lifecycle/src/workspace_session.rs:138`
   - `SessionAgentRecord.agent_id` and `.daemon_instance_id` — written by `roster_record`
     (`packages/tddy-session-agents/src/agent_records.rs:21-36`)

   Field docs are at `packages/tddy-changeset/src/session_metadata.rs:80-105`.

4. **Every read of it then fails the eligible-list check.** `classify_peer_route`
   (`packages/tddy-daemon-kernel/src/peer_forwarding.rs:51-63`) matches the requested id against
   `eligible_instance_ids()`, which is built from the *live* common-room discovery participants
   (`packages/tddy-daemon-livekit/src/peer_routing.rs:65-71`). A restarted daemon advertises the new
   id only. `classify_codebase_placement`
   (`packages/tddy-session-lifecycle/src/connection_service/placement.rs:145-151`) rejects on the
   same basis.

---

## The repo already knows routing ids are per-run — three places missed it

This hazard is documented and solved for the **host registry**.
`local_base_instance_id_for_config` (`packages/tddy-daemon-kernel/src/daemon_identity.rs:50-70`)
exists precisely so durable state keys on the un-suffixed id, and says why in its own doc comment:
keying durable state on the routing id "would file every restart of one machine as a brand-new
host", after which the registry's never-delete rule "would keep every one of those ghosts forever".
`packages/tddy-host-service/src/host_registry.rs:18-23` repeats the reasoning.

Session metadata and the agent roster are durable state of exactly that kind, and key on the
routing id anyway. **The missing piece is the indirection the host registry already has: record the
durable base id, resolve it to the current routing id at the point of use.**

---

## Why no error reaches the operator

Two separate things, and only the second is a reporting gap.

1. **`ResumeSession` never looks at the recorded id at all.**
   `resume_session_at_session_coordinate`
   (`packages/tddy-session-lifecycle/src/connection_service/session_coordinate_handlers/svc_resume_session.rs`)
   dispatches purely on `session_type`, takes the `claude-cli` branch, and resumes the agent half
   locally. It never reads `codebase_daemon_instance_id`, never calls `split_pairing`
   (`packages/tddy-session-lifecycle/src/split_session.rs:80-88`), and returns
   `ResumeSessionResponse` **successfully** for a split session it has no route to the codebase half
   of. Resume is not failing — it is reporting success for a session that is half-resumed and cannot
   work.

2. **The rejection surfaces later, detached from the action that caused it.** The routing RPCs do
   return a real status: `classify_addressed_daemon_route` maps it to `Status::invalid_argument`
   (`packages/tddy-daemon-livekit/src/peer_routing.rs:110-113`), and the streaming roster hook does
   render it verbatim
   (`packages/tddy-web/src/components/sessions/useSessionAgentRoster.ts:74-80`). But it arrives as a
   roster-pane error about an unknown daemon, attributed to nothing the operator did, rather than as
   "Resume failed: this session's codebase host is gone."

> ⚠️ An earlier note in this investigation said the rejection was "logged at INFO and never
> surfaced". That was wrong and is corrected above — it *is* returned as a status. Do not plan a fix
> around surfacing it; plan around (1), which is where the silence actually comes from.

---

## What the fix is

### (b) Stop shipping the flag on — do this first, it is the cheapest and largest

Remove `daemon_instance_id_append_startup_timestamp: true` from `desktop.yaml.production:108`.

The template's own comment (lines 106-107) says the flag is there "so the installed application and
a `./desktop-dev` run on the same machine do not collide with DuplicateIdentity" — that is a
*developer* scenario written into a production template. And the hazard cannot even arise in a
default install: `./install --desktop` renders `livekit:` unset (confirmed — `~/.tddy/desktop.yaml`
has no `livekit:` block), and `DuplicateIdentity` is a common-room collision
(`packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs:47` and the error text at 1275). No
common room, no collision. `dev.desktop.yaml:45` may legitimately keep it — that is the case it was
written for.

**This does not repair installed machines.** `./install --desktop` never overwrites an existing
`~/.tddy/desktop.yaml`, so every already-installed app keeps its line 34 until someone edits it by
hand. Ship a note, or handle it under the migration question below.

### (a) Persist the durable id, resolve at use — the actual fix

Needed because (b) only removes the *common* trigger. A machine that legitimately runs two daemons
still needs the flag, and must still survive a restart.

- Write `codebase_daemon_instance_id`, `agent_daemon_instance_id`, and the `daemon_instance_id`
  inside each `SessionAgentRecord` from `local_base_instance_id_for_config`, not
  `local_instance_id_for_config`.
- Resolve a recorded durable id to the current routing id at every routing point — through the
  eligible list, or through the host registry, which is already keyed durably.
- **`agent_id` is the hard part, not the placement fields.** It is a composed, parsed, persisted key
  (`AgentId { name, daemon_instance_id }`, `@`-separated). Changing what goes into it changes
  existing rosters' keys, so this needs a decision on whether old ids are migrated or resolved
  leniently — and `qualified_agent_id` deliberately refuses a name containing `@`
  (`agent_records.rs:38-42`), which any migration must not break.

### (c) Make Resume refuse a route it does not have

In `resume_session_at_session_coordinate`, when `split_pairing(&metadata)` returns a pair, check the
recorded codebase daemon against the eligible list and fail with the reason — rather than resuming
the agent half into a worktree it cannot reach and answering `OK`. This is worth doing even after
(a) and (b), because a genuinely offline peer is a real state that should read as a clear failure.

---

## Open question for the developer — not decided here

Whether sessions already recorded against a dead id should be **migrated** (rewritten to the durable
id), **repointed** at the current daemon, or **left orphaned**. Every session created by a desktop
install since the flag shipped is affected, including the roster `agent_id`s. The answer decides
whether (a) alone is sufficient or whether a migration ships with it.

## How to reproduce and how to verify a fix

1. Start the desktop app; create a split session (a `claude-cli` session with a
   `codebase_daemon_instance_id`, i.e. managed codebase on another/sandboxed host).
2. `grep daemon_instance_id ~/.tddy/sessions/<id>/.session.yaml` — note the suffixed id.
3. Quit and relaunch the app. `grep daemon_instance_id ~/.tddy/desktop.yaml` confirms the flag;
   the new daemon's id in the log differs only in the trailing milliseconds.
4. Press Resume. Today: the call returns success, and the roster pane shows
   `unknown or not connected daemon_instance_id …`.
5. After a fix: the recorded id survives the restart and the session resumes; or, if the host is
   genuinely gone, Resume itself fails with that reason.
